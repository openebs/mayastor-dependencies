//! FIPS mode for our rustls based TLS stacks.
//!
//! Whether a process *can* do FIPS is settled when it is built: depending on
//! this crate pins rustls and aws-lc-rs to the FIPS validated AWS-LC build
//! (`aws-lc-fips-sys`) for every rustls user in the dependency graph, via
//! cargo's feature unification. Components get that by depending on us, and
//! should not repeat the pins themselves.
//!
//! The one exception is Windows: `aws-lc-fips-sys` only builds against MSVC and
//! cannot be cross-compiled to the windows-gnu target, so the `fips` feature is
//! off there and the standard AWS-LC is used instead. On such a build [`init`]
//! finds no FIPS provider and returns [`Error::NotValidated`], while
//! [`init_if`]`(false)` stays a no-op.
//!
//! What is left to do at runtime is [`init`], as early in `main` as possible.
//!
//! Do not be alarmed by `aws-lc-sys`, the *non* FIPS module, showing up in the
//! build alongside `aws-lc-fips-sys`: rustls' `fips` feature enables its
//! `aws_lc_rs` one, which hard-enables `aws-lc-rs/aws-lc-sys`, and kube,
//! hyper-rustls and async-nats each enable `rustls/aws_lc_rs` besides. There
//! is no way to turn that back off from here. It costs build time only and
//! does not reach the binary: `aws-lc-rs` only ever names `aws_lc_fips_sys`
//! when built with `fips`, so nothing links the other one in. [`enabled`] is
//! what settles which module actually came up, at runtime.
//!
//! Note that components typically build no TLS config of their own: they are
//! built for them by kube-client, async-nats, reqwest and friends, all from
//! the process-wide default [`rustls::crypto::CryptoProvider`]. Installing
//! that provider is therefore the only lever we have over them, and it has to
//! get in before the first config is built - kube-client, for one, installs
//! the plain provider itself and ignores the failure if a default is already
//! set.

/// Errors which can be hit while setting up FIPS mode.
#[derive(Debug)]
pub enum Error {
    /// The linked crypto module is not FIPS validated.
    NotValidated,
    /// A TLS configuration is not FIPS compliant.
    ConfigNotCompliant {
        /// Which side of the connection it configures.
        kind: &'static str,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotValidated => write!(
                f,
                "the linked crypto module is not FIPS validated, \
                this binary was not built for FIPS mode"
            ),
            Self::ConfigNotCompliant { kind } => {
                write!(f, "the {kind} tls configuration is not FIPS compliant")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Installs the FIPS crypto provider as the process-wide rustls default, and
/// verifies that it really is FIPS validated.
///
/// Call this as early in `main` as possible, before anything gets a chance to
/// build a TLS config, and fail startup if it errors: a process which cannot
/// do FIPS must not pretend otherwise.
///
/// It is safe to call more than once, and losing the race to another
/// installer is not an error so long as what won is FIPS validated too, which
/// [`enabled`] is what actually decides.
pub fn init() -> Result<(), Error> {
    // default_fips_provider() only exists with rustls' `fips` feature, which we
    // do not build on Windows (aws-lc-fips-sys cannot be cross-compiled there).
    // Without it nothing installs a FIPS provider, so enabled() stays false and
    // we report NotValidated - which is the truth on a non-FIPS build.
    #[cfg(not(target_os = "windows"))]
    let _ = rustls::crypto::default_fips_provider().install_default();

    if !enabled() {
        return Err(Error::NotValidated);
    }

    Ok(())
}

/// Same as [`init`] but with the enable passed as argument for ease of use.
pub fn init_if(fips: bool) -> Result<(), Error> {
    if !fips {
        return Ok(());
    }
    init()
}

/// Whether the process-wide default crypto provider is FIPS validated.
///
/// This is a runtime check of the linked AWS-LC rather than a compile time
/// constant, so it also tells us the module's self tests passed. It is
/// `false` before [`init`] has installed a default.
pub fn enabled() -> bool {
    rustls::crypto::CryptoProvider::get_default().is_some_and(|provider| provider.fips())
}

/// Asserts that a TLS configuration is FIPS compliant.
///
/// Only needed by components which build their own configs. A config built
/// from the process default provider through the usual rustls builders is
/// compliant by construction, since they derive `require_ems` from the
/// provider, so there is nothing to check for the configs built on our behalf
/// by kube-client, async-nats and the rest.
pub trait FipsConfig {
    /// Errors if this configuration is not FIPS compliant.
    fn check_fips(&self) -> Result<(), Error>;
}

impl FipsConfig for rustls::ClientConfig {
    fn check_fips(&self) -> Result<(), Error> {
        match self.fips() {
            true => Ok(()),
            false => Err(Error::ConfigNotCompliant { kind: "client" }),
        }
    }
}

impl FipsConfig for rustls::ServerConfig {
    fn check_fips(&self) -> Result<(), Error> {
        match self.fips() {
            true => Ok(()),
            false => Err(Error::ConfigNotCompliant { kind: "server" }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    /// A resolver is all a `ServerConfig` needs to be built, no real cert.
    #[derive(Debug)]
    struct NoCerts;
    impl rustls::server::ResolvesServerCert for NoCerts {
        fn resolve(
            &self,
            _: rustls::server::ClientHello<'_>,
        ) -> Option<Arc<rustls::sign::CertifiedKey>> {
            None
        }
    }

    /// Kept as one test: the installed provider is process-wide state, so
    /// separate tests would race each other through it.
    #[test]
    fn fips_mode_is_available() {
        // Nothing is installed until we ask for it.
        assert!(!enabled());

        init().expect("built against the FIPS validated AWS-LC");
        assert!(enabled());

        // Idempotent, even though the second install loses the race.
        init().expect("still in FIPS mode");

        // Configs built the usual way inherit compliance from the provider we
        // just installed, which is what the components who build their own on
        // our behalf rely on.
        rustls::ClientConfig::builder()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth()
            .check_fips()
            .expect("built from the fips provider");

        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(NoCerts))
            .check_fips()
            .expect("built from the fips provider");
    }
}

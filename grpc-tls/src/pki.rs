//! A throwaway PKI for tests and local deployments.
//!
//! This generates a self-signed certificate authority and issues leaf
//! certificates from it, so a cluster can be brought up with file-backed
//! mutual TLS without any external tooling. The material is written as PEM
//! files which are consumed by [`crate::file_client_config`] and
//! [`crate::file_server_config`].
//!
//! Issued certificates carry both the server and client authentication
//! extended key usages so the same identity can serve a gRPC endpoint and
//! authenticate to another.

use crate::{CertOp, Error};
use std::{io, path::Path};

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};

/// A self-signed certificate authority.
pub struct CertificateAuthority {
    certificate: rcgen::Certificate,
    key_pair: KeyPair,
}

/// A certificate issued by a [`CertificateAuthority`], with its private key.
pub struct Certificate {
    certificate_pem: String,
    private_key_pem: String,
}

impl CertificateAuthority {
    /// Generate a new self-signed certificate authority.
    pub fn generate(common_name: &str) -> Result<Self, Error> {
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        let key_pair = KeyPair::generate().map_err(|source| Error::Certificate {
            op: CertOp::GenerateCaKeyPair,
            source,
        })?;
        let certificate = params
            .self_signed(&key_pair)
            .map_err(|source| Error::Certificate {
                op: CertOp::SelfSignCa,
                source,
            })?;

        Ok(Self {
            certificate,
            key_pair,
        })
    }

    /// The CA certificate as PEM, to be used as a trust root (CA bundle).
    pub fn certificate_pem(&self) -> String {
        self.certificate.pem()
    }

    /// Write the CA certificate (PEM) to `path`.
    pub fn write_certificate(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, self.certificate_pem())
    }

    /// Issue a leaf certificate for the given subject alternative names.
    ///
    /// Each name is parsed as an IP address when possible and otherwise used
    /// as a DNS name. The certificate is valid for both server and client
    /// authentication.
    pub fn issue(
        &self,
        common_name: &str,
        subject_alt_names: &[String],
    ) -> Result<Certificate, Error> {
        let mut params = CertificateParams::new(subject_alt_names.to_vec()).map_err(|source| {
            Error::Certificate {
                op: CertOp::SubjectAltNames,
                source,
            }
        })?;
        params.is_ca = IsCa::NoCa;
        params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];

        let key_pair = KeyPair::generate().map_err(|source| Error::Certificate {
            op: CertOp::GenerateKeyPair,
            source,
        })?;
        let certificate = params
            .signed_by(&key_pair, &self.certificate, &self.key_pair)
            .map_err(|source| Error::Certificate {
                op: CertOp::SignCertificate,
                source,
            })?;

        Ok(Certificate {
            certificate_pem: certificate.pem(),
            private_key_pem: key_pair.serialize_pem(),
        })
    }
}

impl Certificate {
    /// The certificate as PEM.
    pub fn certificate_pem(&self) -> &str {
        &self.certificate_pem
    }

    /// The private key as PEM (PKCS#8).
    pub fn private_key_pem(&self) -> &str {
        &self.private_key_pem
    }

    /// Write the certificate and private key (PEM) to the given paths.
    pub fn write(&self, certificate: &Path, private_key: &Path) -> io::Result<()> {
        std::fs::write(certificate, &self.certificate_pem)?;
        std::fs::write(private_key, &self.private_key_pem)
    }
}

#[cfg(test)]
mod tests {
    use super::CertificateAuthority;

    #[test]
    fn issued_certificates_verify_against_the_ca() {
        let ca = CertificateAuthority::generate("test-ca").unwrap();
        let leaf = ca
            .issue("core", &["core".to_string(), "10.1.0.2".to_string()])
            .unwrap();

        let dir = std::env::temp_dir().join(format!("grpc-tls-pki-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ca_path = dir.join("ca.pem");
        let cert_path = dir.join("core.crt");
        let key_path = dir.join("core.key");
        ca.write_certificate(&ca_path).unwrap();
        leaf.write(&cert_path, &key_path).unwrap();

        crate::file_server_config(&cert_path, &key_path, Some(&ca_path)).unwrap();
        crate::file_client_config(Some(&ca_path), Some(&cert_path), Some(&key_path)).unwrap();

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

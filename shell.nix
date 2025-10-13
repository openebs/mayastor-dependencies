{ profile ? "stable", version ? "1.88.0" }:
let
  sources = import ./nix/sources.nix;
  pkgs = import sources.nixpkgs {
    overlays = [ (_: _: { inherit sources; }) (import ./nix/overlay.nix { }) ];
  };
  rust = import sources.nixpkgs { overlays = [ (import sources.rust-overlay) ]; };
  usePreCommit = builtins.getEnv "IN_NIX_SHELL" == "impure" && builtins.getEnv "CI" != "1";
  pre-commit = pkgs.runCommand "pre-commit" { } ''
    mkdir -p $out/bin
    cp ${pkgs.pre-commit}/bin/pre-commit $out/bin/pre-commit
  '';
in
let
  rust-bin =
    (rust.rust-bin.${profile}.${version}.default.override {
      extensions = [ "rust-src" ];
    });
in
with pkgs;
pkgs.mkShell {
  buildInputs = [
    rust-bin
    cacert
    cargo-udeps
    clang
    openssl
    pkg-config
    protobuf
    udev
    util-linux
    commitlint
    git
  ] ++ pkgs.lib.optional (usePreCommit) pre-commit;

  LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
  PROTOC = "${protobuf}/bin/protoc";
  PROTOC_INCLUDE = "${protobuf}/include";

  shellHook = ''
    if [ "${toString usePreCommit}" = "1" ]; then
      pre-commit install
      pre-commit install --hook commit-msg
    fi

    if [ -d ~/.cargo/bin ]; then
      # Adding ~/.cargo/bin to the path let's us carry on using rustup but it lowers its
      # priority: https://github.com/rust-lang/cargo/pull/11023
      export PATH=$PATH:~/.cargo/bin
    fi
  '';
}

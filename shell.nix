{ profile ? "nightly", date ? "2023-08-10" }:
let
  sources = import ./nix/sources.nix;
  pkgs = import sources.nixpkgs {
    overlays = [ (_: _: { inherit sources; }) (import ./nix/overlay.nix { }) ];
  };
  rust = import sources.nixpkgs { overlays = [ (import sources.rust-overlay) ]; };
in
let
  rust-bin =
    (rust.rust-bin.${profile}.${date}.default.override {
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
    pre-commit
    protobuf
    udev
    util-linux
    commitlint
    git
  ];

  LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
  PROTOC = "${protobuf}/bin/protoc";
  PROTOC_INCLUDE = "${protobuf}/include";
  NODE_PATH = "${nodePackages."@commitlint/config-conventional"}/lib/node_modules";

  shellHook = ''
    pre-commit install
    pre-commit install --hook commit-msg
  '';
}

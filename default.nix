{}:
let
  sources = import ./nix/sources.nix;
  pkgs = import sources.nixpkgs {
    overlays = [ (_: _: { inherit sources; }) (import ./nix/overlay.nix { }) ];
  };
in
pkgs

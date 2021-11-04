{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        # What we build and the command.
        the-name = "gandi-dns-update";

        rust = pkgs.rust-bin.stable."1.56.1";
        rust-bin = rust.default;
        rust-src = rust.rust-src;

        myRustPlatform = pkgs.makeRustPlatform {
          cargo = rust-bin;
          rustc = rust-bin;
        };

        cargo-version = (builtins.fromTOML (builtins.readFile ./Cargo.toml) ).package.version;

        gandi-dns-update = myRustPlatform.buildRustPackage rec {
          pname = the-name;
          version = cargo-version;
          src = pkgs.lib.cleanSource ./.;
          cargoLock = { lockFile = ./Cargo.lock; };
          dockCheck = false; # Disable tests.
        };
      in {
        defaultPackage = gandi-dns-update;

        # Nix: nix build .#gandi-dns-update-image && docker load < result
        packages.gandi-dns-update-image = pkgs.dockerTools.buildLayeredImage {
          name = the-name;
          tag = cargo-version;
          contents = [ pkgs.cacert gandi-dns-update ];
          config = {
            Cmd = [ the-name ];
            WorkingDir = "/";
          };
        };

        devShell = pkgs.mkShell {
          buildInputs = [
            rust-bin
            rust-src
            pkgs.rust-analyzer
          ];

          RUST_BACKTRACE=1;
          RUST_SRC="${rust.rust-src}/lib/rustlib/src/rust/library";
        };
      });
}

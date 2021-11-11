{
  inputs = {
    nixpkgs.url = github:nixos/nixpkgs/nixpkgs-unstable;
    # NOTE https://github.com/nix-community/naersk/
    naersk = {
      url = github:nmattia/naersk;
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # NOTE https://github.com/nix-community/fenix/
    fenix = {
      url = github:nix-community/fenix;
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, naersk, fenix, flake-utils, }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        toolchainBase = fenix.packages.${system}.toolchainOf {
          channel = "1.56.1";
          sha256 = "sha256-MJyH6FPVI7diJql9d+pifu5aoqejvvXyJ+6WSJDWaIA=";
        };

        toolchain = with fenix.packages.${system}; combine [
          toolchainBase.rustc
          toolchainBase.cargo
          targets.x86_64-unknown-linux-musl.stable.rust-std
        ];

        naersk-lib = naersk.lib.${system}.override {
          cargo = toolchain;
          rustc = toolchain;
        };

        myname = "gandi-dns-update";
        cargo-version = (builtins.fromTOML (builtins.readFile ./Cargo.toml) ).package.version;

      in rec {
        defaultPackage = packages.x86_64-unknown-linux-musl;

        packages.x86_64-unknown-linux-musl = naersk-lib.buildPackage {
          src = ./.;
          nativeBuildInputs = with pkgs; [ pkgsStatic.stdenv.cc pkgsStatic.pkgconfig ];
          buildInputs = with pkgs; [ pkgsStatic.openssl ];
          # ref: https://doc.rust-lang.org/cargo/reference/config.html#buildtarget
          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          # ref: https://github.com/rust-lang/rust/issues/79624#issuecomment-737415388
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
          # ref: https://doc.rust-lang.org/cargo/reference/config.html#targettriplelinker
          # CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=...
          doCheck = true;
        };

        # NOTE nix build .#gandi-dns-update-image && docker load < result
        packages.gandi-dns-update-image = pkgs.dockerTools.buildLayeredImage {
          name = myname;
          tag = cargo-version;
          contents = [ pkgs.cacert defaultPackage ];
          config = {
            Cmd = [ myname ];
            WorkingDir = "/";
          };
        };

        # TODO RUST_SRC="${rust.rust-src}/lib/rustlib/src/rust/library";
        devShell = pkgs.mkShell {
          nativeBuildInputs = [
            (toolchainBase.withComponents [
              "cargo" "rustc" "rust-src" "rustfmt" "clippy"
            ])
            fenix.packages.${system}.rust-analyzer
          ];
          RUST_BACKTRACE=1;
          RUST_SRC="${toolchainBase.rust-src}/lib/rust-lib/src";
        };
      });
}

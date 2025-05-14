{
  description = "Reproducible Rust cross-build for repro_build";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
  };

outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
  flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
    let
      overlays = [ rust-overlay.overlays.default ];
      pkgs     = import nixpkgs { inherit system overlays; };
    in {
      packages.default = pkgs.rustPlatform.buildRustPackage rec {
        pname     = "repro_build";
        version   = "0.1.0";
        src       = ./.;
        cargoLock = { lockFile = ./Cargo.lock; };
        buildInputs       = with pkgs; [ openssl ];
        nativeBuildInputs = with pkgs; [ pkg-config ];
      };

      devShells.default = pkgs.mkShell {
        buildInputs = [
          pkgs.rust-bin.stable.latest
          pkgs.openssl
          pkgs.pkg-config
        ];
        shellHook = ''
          alias ls=eza
          alias find=fd
        '';
      };
    }
  );

}
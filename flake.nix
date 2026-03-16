{
  description = "A CLI tool to parse Erlang crashdumps";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
  let
    overlays = {
      default = final: prev: {
        crashdump_viewer_cli = self.packages.${final.system}.crashdump_viewer_cli;
      };
    };
  in
  flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
      };

      rustToolchain = pkgs.rust-bin.stable.latest.default.override {
        extensions = [ "rust-src" "rust-analyzer" ];
      };

      nativeBuildInputs = with pkgs; [
        rustToolchain
        pkg-config
      ];

      buildInputs = with pkgs; [
        openssl
      ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
        pkgs.darwin.apple_sdk.frameworks.Security
        pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
      ];

    in rec {
      packages = {
        crashdump_viewer_cli = pkgs.rustPlatform.buildRustPackage {
          pname = "crashdump_viewer_cli";
          version = "0.3.0";
          
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          inherit nativeBuildInputs buildInputs;

          meta = with pkgs.lib; {
            description = "A CLI tool to parse Erlang crashdumps";
            homepage = "https://github.com/WhatsApp/crashdump_viewer_cli";
            license = licenses.asl20;
            maintainers = [];
            mainProgram = "crashdump_viewer_cli";
          };
        };

        default = packages.crashdump_viewer_cli;
      };

      apps = {
        crashdump_viewer_cli = {
          type = "app";
          program = "${packages.crashdump_viewer_cli}/bin/crashdump_viewer_cli";
          meta = packages.crashdump_viewer_cli.meta;
        };
        
        default = apps.crashdump_viewer_cli;
      };

      devShells.default = pkgs.mkShell {
        inherit nativeBuildInputs buildInputs;
        RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
      };
    }
  ) // { inherit overlays; };
}

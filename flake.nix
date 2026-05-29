{
  description = "oxidizedgraph - LangGraph in Rust";

  nixConfig = {
    extra-substituters = [ "https://nix-cache.stevedores.org" ];
    extra-trusted-public-keys = [
      "nix-cache.stevedores.org-1:Y2WLZtQTgxQ2QQzUnRDkDDKX08dL3NoNZ+Ohw3jv+7I="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, crane, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          nativeBuildInputs = with pkgs; [ pkg-config cmake ];
          buildInputs = with pkgs; [ openssl libgit2 ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        oxidizedgraph-server = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          pname = "oxidizedgraph-server";
          cargoExtraArgs = "--bin oxidizedgraph-server";
        });

        server-image = pkgs.dockerTools.buildLayeredImage {
          name = "oxidizedgraph/server";
          tag = "latest";
          contents = [ oxidizedgraph-server pkgs.cacert ];
          config = {
            Cmd = [ "${oxidizedgraph-server}/bin/oxidizedgraph-server" ];
            ExposedPorts = { "8080/tcp" = {}; };
            Env = [ "PORT=8080" "RUST_LOG=info" ];
          };
        };
      in
      {
        packages = {
          default = oxidizedgraph-server;
          inherit oxidizedgraph-server server-image;
        };

        checks = {
          inherit oxidizedgraph-server;
          clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- -D warnings";
          });
          fmt = craneLib.cargoFmt { src = craneLib.cleanCargoSource ./.; };
          tests = craneLib.cargoNextest (commonArgs // {
            inherit cargoArtifacts;
            partitions = 1;
            partitionType = "count";
          });
        };

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          packages = with pkgs; [
            rustToolchain
            cargo-watch
            surrealdb
            skopeo
            just
            git
          ];
          RUST_BACKTRACE = "1";
          shellHook = ''
            echo "oxidizedgraph dev shell"
            echo "  nix build .#server-image"
            echo "  kubectl apply -k deploy/overlays/gke-autopilot"
          '';
        };
      }
    );
}

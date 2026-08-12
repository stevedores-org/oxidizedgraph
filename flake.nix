{
  description = "oxidizedgraph - LangGraph in Rust";

  # Stevedores binary cache is opt-in (see docs/PACKAGING.md). nixConfig here is not
  # applied unless the user is a trusted-user or passes --accept-flake-config.

  inputs = {
    nixpkgs.url = "tarball+https://registry.aivcs.io/github/NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "tarball+https://registry.aivcs.io/github/oxalica/rust-overlay";
    flake-utils.url = "tarball+https://registry.aivcs.io/github/numtide/flake-utils";
    crane.url = "tarball+https://registry.aivcs.io/github/ipetkov/crane";
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
        imageTag = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          nativeBuildInputs = with pkgs; [ pkg-config cmake ];
          # openssl-sys on Darwin: nixpkgs openssl is usually enough. If a future
          # nixpkgs bump requires Apple frameworks, add them via the current
          # apple-sdk / darwin SDK docs (legacy darwin.apple_sdk.* was removed).
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
          tag = imageTag;
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
            kubectl
            kustomize
            kubeconform
            just
            git
          ];
          RUST_BACKTRACE = "1";
          shellHook = ''
            echo "oxidizedgraph dev shell"
            echo "  nix build .#server-image"
            echo "  kubectl kustomize deploy/overlays/gke-autopilot"
            echo "  kubeconform -kubernetes-version 1.29.0 <(kubectl kustomize deploy/overlays/gke-autopilot)"
          '';
        };
      }
    );
}

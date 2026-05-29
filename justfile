# oxidizedgraph — common workflows

default:
    @just --list

build:
    cargo build --bin oxidizedgraph-server

test:
    cargo test

run:
    cargo run --bin oxidizedgraph-server

# OCI image via Nix (no Dockerfile).
image:
    nix build .#server-image -L

push:
    skopeo copy docker-archive:./result \
      docker://ghcr.io/stevedores-org/oxidizedgraph/server:latest

images:
    dockworker build

deploy-gke:
    kubectl apply -k deploy/overlays/gke-autopilot

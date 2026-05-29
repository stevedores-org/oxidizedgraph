# oxidizedgraph — common workflows

default:
    @just --list

image-tag := env_var_or_default("IMAGE_TAG", "0.2.0")
image := "ghcr.io/stevedores-org/oxidizedgraph/server:" + image-tag

build:
    cargo build --bin oxidizedgraph-server

test:
    cargo test

run:
    cargo run --bin oxidizedgraph-server

# OCI image via Nix (no Dockerfile).
image:
    nix build .#server-image -L

push: image
    skopeo copy docker-archive:./result "docker://{{image}}"

images:
    dockworker build

kustomize-check:
    kubectl kustomize deploy/overlays/gke-autopilot > /dev/null

deploy-gke:
    kubectl apply -k deploy/overlays/gke-autopilot

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

# ECR image built by CI (linux/amd64). Override registry/repo via env.
ecr-registry := env_var_or_default("ECR_REGISTRY", "148080843892.dkr.ecr.us-east-2.amazonaws.com")
ecr-image := ecr-registry + "/stevedores-org/oxidizedgraph/server:" + image-tag

deploy-eks:
    kubectl apply -k deploy/overlays/gke-autopilot
    kubectl -n oxidizedgraph set image deployment/oxidizedgraph oxidizedgraph={{ecr-image}}
    kubectl -n oxidizedgraph rollout status deployment/oxidizedgraph

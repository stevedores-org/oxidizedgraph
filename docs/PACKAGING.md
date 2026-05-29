# Packaging

Container images are built with **Nix** (`flake.nix` + crane) and pushed with **skopeo** / **dockworker** (`dockworker.toml`). No Dockerfiles for production.

## Build one image

```bash
nix build .#server-image -L
# OCI tarball at ./result — tag is 0.2.0 (matches Cargo.toml)
```

```bash
just image
just push                    # IMAGE_TAG defaults to 0.2.0
IMAGE_TAG="$(git rev-parse --short HEAD)" just push
```

## Stevedores binary cache (opt-in)

`flake.nix` does **not** set `nixConfig.extra-substituters` (it only applies for trusted users or with `--accept-flake-config`, and otherwise warns on every `nix` invocation).

To use `https://nix-cache.stevedores.org` locally, pick one:

```bash
# One-shot
nix build .#server-image --accept-flake-config \
  --extra-substituters https://nix-cache.stevedores.org \
  --extra-trusted-public-keys 'nix-cache.stevedores.org-1:Y2WLZtQTgxQ2QQzUnRDkDDKX08dL3NoNZ+Ohw3jv+7I='

# Persistent (user nix.conf)
mkdir -p ~/.config/nix
cat >> ~/.config/nix/nix.conf <<'EOF'
extra-substituters = https://nix-cache.stevedores.org
extra-trusted-public-keys = nix-cache.stevedores.org-1:Y2WLZtQTgxQ2QQzUnRDkDDKX08dL3NoNZ+Ohw3jv+7I=
EOF
```

CI runners should configure the same substituters in workflow `nix.conf`, not in the flake.

## macOS / openssl-sys

`nix build .#server-image` on Darwin uses Nixpkgs `openssl` (no vendored `apple_sdk` stubs — those were removed upstream). If a future `nixpkgs` bump breaks `openssl-sys`, add the current [Darwin SDK frameworks](https://nixos.org/manual/nixpkgs/stable/#sec-darwin-legacy-frameworks) to `flake.nix` `buildInputs`.

## dockworker.toml tags

- `[tags].default` and per-image `tag` track the release semver (`0.2.0`).
- Override at push time with `IMAGE_TAG` (git SHA recommended for prod).

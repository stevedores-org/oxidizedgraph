#!/usr/bin/env bash
# Provision Cloudflare Access (interactive SSO) for paid commercial docs.
#
# Desired state: deploy/cloudflare-access/configmap.yaml
# Runbook: docs/RUNBOOK_COMMERCIAL_DOCS_SSO.md
#
# Requires Account API token with Access: Apps/Policies → Edit.
# OAuth wrangler tokens (cfat_/cfoat_) will NOT work.
#
# Usage:
#   export CF_API_TOKEN=<account api token>
#   ./scripts/provision-cloudflare-docs-access.sh [--dry-run] [--verify-only]
#
set -euo pipefail

CONFIG_MAP="cloudflare-access-oxidizedgraph-docs"
NAMESPACE="crossplane-system"
DRY_RUN=false
VERIFY_ONLY=false
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --config-map) CONFIG_MAP="$2"; shift 2 ;;
    --namespace) NAMESPACE="$2"; shift 2 ;;
    --dry-run) DRY_RUN=true; shift ;;
    --verify-only) VERIFY_ONLY=true; shift ;;
    -h|--help)
      sed -n '2,14p' "$0"
      exit 0
      ;;
    *) echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

if ! command -v jq >/dev/null; then
  echo "jq is required" >&2
  exit 1
fi

load_config() {
  if kubectl get cm "$CONFIG_MAP" -n "$NAMESPACE" >/dev/null 2>&1; then
    kubectl get cm "$CONFIG_MAP" -n "$NAMESPACE" -o json | \
      jq -r '.data | to_entries[] | "\(.key)=\(.value)"' > /tmp/cf-docs-access.env
    set -a
    # shellcheck disable=SC1091
    source /tmp/cf-docs-access.env
    set +a
  elif [[ -f "${REPO_ROOT}/deploy/cloudflare-access/configmap.yaml" ]]; then
    account_id="${CLOUDFLARE_ACCOUNT_ID:-}"
    access_app_name="oxidizedgraph-docs"
    access_app_hostname="docs.oxidizedgraph.lornu.ai"
    staff_email_domain="lornu.ai"
    allowed_idps="google,github"
    session_duration="8h"
  else
    echo "ConfigMap $CONFIG_MAP not found and no local fallback" >&2
    exit 1
  fi

  : "${account_id:?account_id missing — set CLOUDFLARE_ACCOUNT_ID or ConfigMap}"
  : "${access_app_name:?access_app_name missing}"
  : "${access_app_hostname:?access_app_hostname missing}"
}

verify_gate() {
  echo "==> Verifying Cloudflare Access SSO gate on https://${access_app_hostname}/"
  local code
  code=$(curl -sS -o /dev/null -w '%{http_code}' "https://${access_app_hostname}/" || true)
  echo "    Unauthenticated browser: HTTP $code (expect 302 to IdP or 403 when Access active)"
  case "$code" in
    403|302) echo "    OK: hostname requires Cloudflare Access" ;;
    200) echo "    FAIL: docs are publicly readable without SSO" >&2; return 1 ;;
    502|503|504) echo "    WARN: origin not ready ($code) but Access may still be configured" ;;
    *) echo "    WARN: unexpected status $code" >&2; return 1 ;;
  esac
}

load_config

if $VERIFY_ONLY; then
  verify_gate
  exit 0
fi

if $DRY_RUN; then
  echo "[dry-run] would provision SSO Access for ${access_app_hostname}"
  echo "  account_id=${account_id}"
  echo "  app=${access_app_name}"
  echo "  idps=${allowed_idps:-google}"
  echo "  staff_domain=${staff_email_domain:-lornu.ai}"
  echo "  customer_group=${customer_access_group_id:-<unset>}"
  exit 0
fi

if [[ -z "${CF_API_TOKEN:-}" ]]; then
  echo "CF_API_TOKEN is required (Access: Apps/Policies → Edit)" >&2
  exit 1
fi

if [[ "$CF_API_TOKEN" == cfat_* || "$CF_API_TOKEN" == cfoat_* ]]; then
  echo "OAuth tokens cannot call the Access API. Use an Account API token." >&2
  exit 1
fi

CF_API="https://api.cloudflare.com/client/v4/accounts/${account_id}/access"

cf_get() {
  curl -fsS "${CF_API}${1}" -H "Authorization: Bearer ${CF_API_TOKEN}"
}

cf_post() {
  local path="$1"
  local body="$2"
  curl -fsS -X POST "${CF_API}${path}" \
    -H "Authorization: Bearer ${CF_API_TOKEN}" \
    -H "Content-Type: application/json" \
    -d "$body"
}

echo "==> Cloudflare Access SSO for commercial docs: ${access_app_hostname}"

APP_ID=$(cf_get "/apps" | jq -r --arg h "$access_app_hostname" \
  '.result[] | select(.domain == $h) | .id' | head -1)

if [[ -n "$APP_ID" ]]; then
  echo "    Access app exists: $APP_ID"
else
  idp_json=$(jq -n --arg idps "${allowed_idps:-google}" \
    '$idps | split(",") | map(select(length > 0))')
  app_body=$(jq -n \
    --arg name "$access_app_name" \
    --arg domain "$access_app_hostname" \
    --arg dur "${session_duration:-8h}" \
    --argjson idps "$idp_json" \
    '{name: $name, domain: $domain, type: "self_hosted", session_duration: $dur, allowed_idps: $idps}')
  APP_ID=$(cf_post "/apps" "$app_body" | jq -r '.result.id')
  echo "    Created Access app: $APP_ID"
fi

ensure_policy() {
  local name="$1"
  local body="$2"
  local existing
  existing=$(cf_get "/apps/${APP_ID}/policies" | jq -r --arg n "$name" \
    '.result[] | select(.name == $n) | .id' | head -1)
  if [[ -n "$existing" ]]; then
    echo "    Policy exists: $name ($existing)"
  else
    cf_post "/apps/${APP_ID}/policies" "$body" >/dev/null
    echo "    Created policy: $name"
  fi
}

if [[ -n "${staff_email_domain:-}" ]]; then
  staff_body=$(jq -n \
    --arg domain "$staff_email_domain" \
    '{name: "Lornu staff", decision: "allow", include: [{email_domain: {domain: $domain}}], precedence: 1}')
  ensure_policy "Lornu staff" "$staff_body"
fi

if [[ -n "${customer_access_group_id:-}" ]]; then
  group_body=$(jq -n \
    --arg gid "$customer_access_group_id" \
    '{name: "Paid customers (Access group)", decision: "allow", include: [{group: {id: $gid}}], precedence: 2}')
  ensure_policy "Paid customers (Access group)" "$group_body"
fi

if [[ -n "${customer_emails:-}" ]]; then
  IFS=',' read -ra CUSTOMERS <<< "$customer_emails"
  for email in "${CUSTOMERS[@]}"; do
    [[ -z "$email" ]] && continue
    cust_body=$(jq -n \
      --arg e "$email" \
      '{name: ("Paid customer (" + $e + ")"), decision: "allow", include: [{email: {email: $e}}], precedence: 3}')
    ensure_policy "Paid customer ($email)" "$cust_body"
  done
fi

# Explicit deny-all fallback for anyone not matched (Access evaluates allow policies first).
deny_body='{"name":"Deny everyone else","decision":"deny","include":[{"everyone":{}}],"precedence":100}'
ensure_policy "Deny everyone else" "$deny_body"

verify_gate || true

echo ""
echo "Done. Add customers via Zero Trust → Groups or customer_emails in ConfigMap."
echo "Docs URL (SSO): https://${access_app_hostname}/"

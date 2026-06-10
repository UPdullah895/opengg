#!/usr/bin/env bash
# validate-extension.sh — Validate an OpenGG extension directory
#
# Usage:
#   scripts/validate-extension.sh <DIR>
#   make validate-extension DIR=<DIR>
#
# Checks:
#   - manifest.json exists and parses
#   - Required fields: id, name, description
#   - id matches kebab-case pattern: ^[a-z0-9]+(-[a-z0-9]+)*$
#   - version (if present) is valid semver
#   - daemon path (if present) has no leading / or .. segments
#   - daemon file exists and is executable (if declared)
#   - main file exists (if declared)
#   - locale files (en.json, ar.json) parse as JSON (if referenced)
#   - Validates against manifest.schema.json if jsonschema is available
#
# Exit code:
#   0 = all checks pass
#   1 = any check fails

set -uo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

DIR="${1:-.}"
MANIFEST="$DIR/manifest.json"

# Track pass/fail
PASS=0
FAIL=0

warn() {
    local message="$1"
    echo -e "${YELLOW}⚠${NC} $message"
}

pass() {
    local message="$1"
    echo -e "${GREEN}✓${NC} $message"
    ((PASS++))
}

fail() {
    local message="$1"
    echo -e "${RED}✗${NC} $message"
    ((FAIL++))
}

# ── 1. Manifest file exists ──────────────────────────────────────────────────
if [[ ! -f "$MANIFEST" ]]; then
    echo -e "${RED}✗${NC} manifest.json not found at $MANIFEST"
    exit 1
fi
pass "manifest.json found"

# ── 2. Parse manifest.json ───────────────────────────────────────────────────
if ! python3 -c "import json; json.load(open('$MANIFEST'))" 2>/dev/null; then
    echo -e "${RED}✗${NC} manifest.json does not parse as valid JSON"
    exit 1
fi
pass "manifest.json is valid JSON"

# Extract fields using python3 for reliable JSON parsing
get_field() {
    local field="$1"
    python3 -c "import json; data=json.load(open('$MANIFEST')); print(data.get('$field', ''))" 2>/dev/null || echo ""
}

ID=$(get_field "id")
NAME=$(get_field "name")
DESCRIPTION=$(get_field "description")
VERSION=$(get_field "version")
DAEMON=$(get_field "daemon")
MAIN=$(get_field "main")

# ── 3. Required fields ───────────────────────────────────────────────────────
if [[ -n "$NAME" ]]; then
    pass "name field is present and non-empty"
else
    fail "name field is present and non-empty"
fi

if [[ -n "$DESCRIPTION" ]]; then
    pass "description field is present and non-empty"
else
    fail "description field is present and non-empty"
fi

# ── 4. ID validation (if present) ────────────────────────────────────────────
if [[ -n "$ID" ]]; then
    if [[ "$ID" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]]; then
        pass "id matches kebab-case pattern (^[a-z0-9]+(-[a-z0-9]+)*\$)"
    else
        fail "id matches kebab-case pattern (^[a-z0-9]+(-[a-z0-9]+)*\$)"
    fi
else
    warn "id field not present (will be derived from folder name at runtime)"
fi

# ── 5. Version validation (if present) ───────────────────────────────────────
if [[ -n "$VERSION" ]]; then
    SEMVER_PATTERN="^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)(-[a-zA-Z0-9]+(\\.[a-zA-Z0-9]+)*)?(\\+[a-zA-Z0-9]+(\\.[a-zA-Z0-9]+)*)?$"
    if [[ "$VERSION" =~ $SEMVER_PATTERN ]]; then
        pass "version is valid semver: $VERSION"
    else
        fail "version is valid semver: $VERSION"
    fi
fi

# ── 6. Description length ────────────────────────────────────────────────────
DESC_LEN=${#DESCRIPTION}
if [[ $DESC_LEN -le 120 ]]; then
    pass "description length is ≤120 chars (current: $DESC_LEN)"
else
    fail "description length is ≤120 chars (current: $DESC_LEN)"
fi

# ── 7. Daemon path validation ────────────────────────────────────────────────
if [[ -n "$DAEMON" ]]; then
    # Check: no leading slash
    if [[ ! "$DAEMON" =~ ^/ ]]; then
        pass "daemon path does not start with '/' (no absolute paths)"
    else
        fail "daemon path does not start with '/' (no absolute paths)"
    fi

    # Check: no .. segments
    if [[ ! "$DAEMON" =~ \.\./|^\.\.$ ]]; then
        pass "daemon path contains no '..' segments"
    else
        fail "daemon path contains no '..' segments"
    fi

    # Check: daemon file exists
    DAEMON_PATH="$DIR/$DAEMON"
    if [[ -f "$DAEMON_PATH" ]]; then
        pass "daemon file exists: $DAEMON"

        # Check: daemon is executable
        if [[ -x "$DAEMON_PATH" ]]; then
            pass "daemon file is executable"
        else
            fail "daemon file is executable"
        fi
    else
        fail "daemon file exists: $DAEMON_PATH"
    fi
fi

# ── 8. Main file validation ──────────────────────────────────────────────────
if [[ -n "$MAIN" ]]; then
    MAIN_PATH="$DIR/$MAIN"
    if [[ -f "$MAIN_PATH" ]]; then
        pass "main file exists: $MAIN"
    else
        warn "main file not found: $MAIN_PATH (build the extension with 'npm run build' to create it)"
    fi
fi

# ── 9. Locale file validation ────────────────────────────────────────────────
for lang in en ar; do
    LOCALE_PATH="$DIR/locales/$lang.json"
    if [[ -f "$LOCALE_PATH" ]]; then
        if python3 -c "import json; json.load(open('$LOCALE_PATH'))" 2>/dev/null; then
            pass "locales/$lang.json is valid JSON"
        else
            fail "locales/$lang.json is valid JSON"
        fi
    fi
done

# ── 10. Schema validation (if jsonschema available) ──────────────────────────
SCHEMA_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/extension-template/manifest.schema.json"
if python3 -c "import jsonschema" 2>/dev/null && [[ -f "$SCHEMA_PATH" ]]; then
    if python3 << EOF 2>/dev/null
import json
import jsonschema
try:
    with open('$MANIFEST') as f:
        manifest = json.load(f)
    with open('$SCHEMA_PATH') as f:
        schema = json.load(f)
    jsonschema.validate(manifest, schema)
except Exception:
    exit(1)
EOF
    then
        pass "manifest validates against manifest.schema.json"
    else
        fail "manifest validates against manifest.schema.json"
    fi
elif python3 -c "import jsonschema" 2>/dev/null; then
    warn "manifest.schema.json not found; skipping schema validation"
else
    warn "jsonschema not available (python3 -m pip install jsonschema); falling back to basic validation"
fi

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "─────────────────────────────────────────────────────────────────"
if [[ $FAIL -eq 0 ]]; then
    echo -e "${GREEN}✓ All checks passed ($PASS/$((PASS + FAIL)))${NC}"
    exit 0
else
    echo -e "${RED}✗ Some checks failed ($PASS passed, $FAIL failed)${NC}"
    exit 1
fi

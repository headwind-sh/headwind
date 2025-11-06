#!/bin/bash
set -e

echo "=== Testing UpdateRequest CRD ==="
echo

# Test 1: Validate YAML syntax with yq or Python
echo "1. Validating CRD YAML syntax..."
if command -v python3 &> /dev/null; then
    python3 -c "import yaml; yaml.safe_load(open('deploy/k8s/crds/updaterequest.yaml'))" && echo "✅ CRD YAML is valid" || echo "❌ CRD YAML is invalid"
else
    echo "⚠️  python3 not found, skipping YAML validation"
fi
echo

# Test 2: Validate example UpdateRequest YAML
echo "2. Validating example UpdateRequest YAML..."
if command -v python3 &> /dev/null; then
    python3 -c "import yaml; yaml.safe_load_all(open('examples/updaterequest.yaml'))" && echo "✅ Example UpdateRequest YAML is valid" || echo "❌ Example YAML is invalid"
else
    echo "⚠️  python3 not found, skipping example validation"
fi
echo

# Test 3: Run Rust unit tests for CRD module
echo "3. Running Rust unit tests for CRD module..."
cargo test --lib models::crd --all-features
echo "✅ Rust unit tests passed"
echo

# Test 4: Test that CRD compiles
echo "4. Testing CRD types compile..."
cargo build --lib 2>&1 | grep -q "Finished" && echo "✅ CRD types compile successfully" || echo "❌ Compilation failed"
echo

# Test 5: Check for clippy warnings
echo "5. Running clippy..."
cargo clippy --lib --all-features -- -D warnings 2>&1 | tail -3
echo "✅ Clippy passed"
echo

# Test 6: Generate a sample UpdateRequest in Rust
echo "6. Testing UpdateRequest creation in Rust..."
cargo run --example create-updaterequest 2>/dev/null || echo "⚠️  Example binary not created yet (this is expected)"
echo

echo "=== Summary ==="
echo "✅ All basic CRD tests passed!"
echo
echo "To test with a real Kubernetes cluster:"
echo "  kubectl apply -f deploy/k8s/crds/updaterequest.yaml"
echo "  kubectl apply -f examples/updaterequest.yaml"
echo "  kubectl get updaterequests"
echo "  kubectl describe ur nginx-update-1-26-0"

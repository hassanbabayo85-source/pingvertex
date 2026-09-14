echo
echo "===== Remaining PostgresApiFlowRepository::new call sites ====="
grep -rn --include='*.rs' 'PostgresApiFlowRepository::new' crates apps || true

echo
echo "===== Remaining direct reads of step.headers as tuples ====="
grep -rn --include='*.rs' 'for (key, value_template) in &step.headers' crates || true

echo
echo "===== cargo fmt ====="
cargo fmt --all

echo
echo "===== cargo check -p domain -p application -p infra -p api (NOT --workspace, NOT --all-targets) ====="
cargo check -p domain -p application -p infra -p api

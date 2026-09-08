# Chain specification artifacts

Generated chain specifications are release artifacts, not source files.

The Stage-1 release workflow generates `stage1.json` and `stage1-raw.json`
from the exact candidate node image, records their SHA-256 values in the
release manifest, and uploads them with the release metadata. Deployments must
use the generated artifact that matches the node image digest.

Do not commit generated chain specifications here. Local and CI commands may
write temporary files into this directory or into a release workspace.

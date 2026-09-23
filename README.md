# plugin-backstage-catalog

Basalt domain-index plugin for Spotify Backstage `catalog-info.yaml` files.

Validates Backstage entity descriptors and emits API index entries so Basalt
can cross-reference catalog entities with TypeScript code.

## Provides
- `backstage-catalog@v1`

## File globs
- `**/catalog-info.yaml`
- `**/catalog-info.yml`

## Capabilities
- `CAP_API_INDEX` — emits one index entry per catalog entity
- `CAP_DIAGNOSTICS` — validates required fields and known `kind` values
- `CAP_CAPABILITY_HANDLE` — native capability dispatch

## Validated fields
- `apiVersion` — must be `"backstage.io/v1alpha1"` or `"backstage.io/v1beta1"`
- `kind` — must be one of: `Component`, `API`, `Group`, `User`, `Resource`, `Location`, `System`, `Domain`, `Template`
- `metadata.name` — required, non-empty
- `metadata.namespace` — optional, warns if absent

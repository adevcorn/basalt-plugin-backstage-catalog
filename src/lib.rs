//! Backstage catalog plugin for Basalt.
//!
//! Validates `catalog-info.yaml` entity descriptors and emits API index entries
//! linking catalog entities to TypeScript source code.

use basalt_plugin_sdk::prelude::*;

basalt_plugin_meta! {
    name:              "backstage-catalog",
    version:           env!("CARGO_PKG_VERSION"),
    hook_flags:        CAP_API_INDEX | CAP_DIAGNOSTICS | CAP_CAPABILITY_HANDLE,
    provides:          "backstage-catalog@v1",
    requires:          "",
    optional_requires: "",
    file_globs:        "**/catalog-info.yaml\n**/catalog-info.yml",
    activates_on:      "**/catalog-info.yaml\n**/catalog-info.yml",
    activation_events: "",
}

// ── Valid values ─────────────────────────────────────────────────────────────

const VALID_API_VERSIONS: &[&str] = &[
    "backstage.io/v1alpha1",
    "backstage.io/v1beta1",
    "backstage.io/v1beta2",
    "backstage.io/v1beta3",
];

const VALID_KINDS: &[&str] = &[
    "Component", "API", "Group", "User", "Resource",
    "Location", "System", "Domain", "Template",
];

// ── Entity model ─────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct CatalogEntity {
    #[serde(rename = "apiVersion")]
    api_version: Option<String>,
    kind: Option<String>,
    metadata: Option<EntityMetadata>,
}

#[derive(serde::Deserialize)]
struct EntityMetadata {
    name: Option<String>,
    namespace: Option<String>,
}

// ── Capability handle ─────────────────────────────────────────────────────────
//
// Wire format (same as semantic-facts): [src_len: u32 LE][src bytes]
// Returns a packed i64: (ptr << 32) | len  — the host reads `len` bytes at `ptr`.

#[unsafe(no_mangle)]
pub extern "C" fn basalt_capability_handle(
    _cap_ptr: *const u8,
    _cap_len: usize,
    req_ptr: *const u8,
    req_len: usize,
) -> i64 {
    if req_ptr.is_null() || req_len == 0 {
        return pack_empty();
    }
    let request = unsafe { core::slice::from_raw_parts(req_ptr, req_len) };

    if request.len() < 4 {
        return pack_error(-1002);
    }
    let src_len = u32::from_le_bytes([request[0], request[1], request[2], request[3]]) as usize;
    if request.len() < 4 + src_len {
        return pack_error(-1002);
    }
    let src = &request[4..4 + src_len];
    if src.is_empty() {
        return pack_empty();
    }

    // Parse and build the API index JSON entry.
    match build_index_entry(src) {
        Some(json) => pack_success(json),
        None => pack_empty(),
    }
}

// ── Diagnostics hook ─────────────────────────────────────────────────────────

#[basalt_plugin]
fn diagnose(src: &[u8], _path: &str) -> Vec<Diagnostic> {
    validate_catalog_yaml(src)
}

// ── Validation ───────────────────────────────────────────────────────────────

fn validate_catalog_yaml(src: &[u8]) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // 1. UTF-8 check.
    let text = match std::str::from_utf8(src) {
        Ok(t) => t,
        Err(_) => {
            diags.push(Diagnostic::new(0, 0, "file is not valid UTF-8", Severity::Error));
            return diags;
        }
    };

    // 2. YAML parse.
    let entity: CatalogEntity = match serde_yaml::from_str(text) {
        Ok(e) => e,
        Err(e) => {
            diags.push(Diagnostic::new(0, 0, format!("YAML parse error: {e}"), Severity::Error));
            return diags;
        }
    };

    // 3. apiVersion.
    match &entity.api_version {
        None => {
            diags.push(Diagnostic::new(0, 0, "missing required field 'apiVersion'", Severity::Error));
        }
        Some(v) if !VALID_API_VERSIONS.contains(&v.as_str()) => {
            let (offset, length) = find_field_span(src, "apiVersion");
            diags.push(Diagnostic::new(
                offset,
                length,
                format!(
                    "unknown apiVersion '{}'; expected one of: {}",
                    v,
                    VALID_API_VERSIONS.join(", ")
                ),
                Severity::Warning,
            ));
        }
        _ => {}
    }

    // 4. kind.
    match &entity.kind {
        None => {
            diags.push(Diagnostic::new(0, 0, "missing required field 'kind'", Severity::Error));
        }
        Some(k) if !VALID_KINDS.contains(&k.as_str()) => {
            let (offset, length) = find_field_span(src, "kind");
            diags.push(Diagnostic::new(
                offset,
                length,
                format!(
                    "unknown kind '{}'; valid kinds are: {}",
                    k,
                    VALID_KINDS.join(", ")
                ),
                Severity::Warning,
            ));
        }
        _ => {}
    }

    // 5. metadata.name.
    let name_missing = entity
        .metadata
        .as_ref()
        .map(|m| m.name.as_deref().unwrap_or("").is_empty())
        .unwrap_or(true);
    if name_missing {
        let (offset, length) = find_field_span(src, "name");
        diags.push(Diagnostic::new(
            offset,
            length,
            "missing required field 'metadata.name'",
            Severity::Error,
        ));
    }

    // 6. metadata.namespace — informational.
    let namespace_missing = entity
        .metadata
        .as_ref()
        .map(|m| m.namespace.is_none())
        .unwrap_or(true);
    if namespace_missing {
        diags.push(Diagnostic::new(
            0,
            0,
            "namespace not set, defaults to 'default'",
            Severity::Info,
        ));
    }

    diags
}

// ── API index builder ─────────────────────────────────────────────────────────

fn build_index_entry(src: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(src).ok()?;
    let entity: CatalogEntity = serde_yaml::from_str(text).ok()?;

    let kind = entity.kind.as_deref().unwrap_or("Unknown");
    let name = entity
        .metadata
        .as_ref()
        .and_then(|m| m.name.as_deref())
        .unwrap_or("");
    let namespace = entity
        .metadata
        .as_ref()
        .and_then(|m| m.namespace.as_deref())
        .unwrap_or("default");
    let catalog_version = entity.api_version.as_deref().unwrap_or("");

    let json = serde_json::json!({
        "kind": kind,
        "name": name,
        "namespace": namespace,
        "catalog_version": catalog_version,
    });

    serde_json::to_vec(&json).ok()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Return `(byte_offset, length)` of the first occurrence of `field_name` in
/// `src` as a UTF-8 byte search, or `(0, 0)` if not found.
fn find_field_span(src: &[u8], field_name: &str) -> (u32, u32) {
    let needle = field_name.as_bytes();
    if let Some(pos) = src
        .windows(needle.len())
        .position(|w| w == needle)
    {
        (pos as u32, needle.len() as u32)
    } else {
        (0, 0)
    }
}

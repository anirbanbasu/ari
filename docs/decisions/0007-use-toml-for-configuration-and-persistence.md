---
parent: Decisions
nav_order: 7
# These are optional metadata elements. Feel free to remove any of them.
status: "accepted"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use TOML for configuration and persistence

## Context and Problem Statement

ARI Inter-Process Communication (IPC) Processes require configuration for operational parameters (IPC Process name, Distributed IPC Facility membership, network bindings, enrolment settings, routing policies) and persistence mechanisms for dynamic state (route snapshots, Resource Information Base snapshots). Configuration must be human-readable for operators, support structured data with sections and nested fields, provide sensible defaults, and enable validation. Persistence snapshots must be versionable, inspectable for debugging, and recoverable after process restarts. We need a format that balances human readability, machine parseability, type safety through schema validation, and Rust ecosystem integration.

## Considered Options

* Use TOML (Tom's Obvious, Minimal Language) for both configuration files and route snapshots.
* Use YAML (YAML Ain't Markup Language) for configuration and persistence.
* Use JSON (JavaScript Object Notation) for configuration and persistence.
* Use JSON5 (JSON for Humans) for improved human-friendliness.
* Use INI files for simple configuration with external format for complex persistence.
* Use environment variables for configuration and binary formats (bincode) for persistence.

## Decision Outcome

Chosen option: "Use TOML for both configuration files and route snapshots", because it provides excellent human readability with minimal syntax noise, strong typing with explicit data types, hierarchical structure through sections (`[section]`, `[[array]]`), seamless Rust integration via `serde` and the `toml` crate, and proven suitability for configuration files in the Rust ecosystem (Cargo itself uses TOML). TOML's strict syntax prevents ambiguities common in YAML whilst remaining more readable than JSON, making it ideal for both static configuration (`bootstrap.toml`, `member.toml`) and inspectable persistence snapshots (`dynamic-routes.toml`).

## Pros and Cons of the Options

### Use TOML for both configuration files and route snapshots

* Good, because it has minimal syntax overhead (no braces, minimal quotes, intuitive sections).
* Good, because it provides explicit typing: strings, integers, booleans, floats, dates, arrays, and tables are unambiguous.
* Good, because hierarchical sections (`[ipcp]`, `[dif]`, `[enrollment]`) map naturally to Rust structs via Serde.
* Good, because array-of-tables syntax (`[[routes]]`) elegantly represents collections (e.g., multiple route entries).
* Good, because it is widely adopted in the Rust ecosystem (Cargo, rustfmt, clippy use `Cargo.toml`).
* Good, because the `toml` crate (v0.8) provides excellent Serde integration with clear error messages for parsing failures.
* Good, because comments are simple (`#`) and encouraged, enabling inline documentation in config files.
* Good, because defaults can be implemented via Serde's `#[serde(default)]` attribute, allowing optional fields with fallback values.
* Good, because route snapshots saved as TOML are human-inspectable for debugging (operators can see learned routes without custom tooling).
* Neutral, because TOML files can become verbose for deeply nested structures, though this is rare in ARI's flat configuration schema.
* Bad, because multiline strings require triple quotes (`"""..."""`), which may be unfamiliar to some users.
* Bad, because TOML does not support null/nil values (fields must be omitted or use `Option<T>` in Rust), requiring explicit handling of optional fields.

### Use YAML for configuration and persistence

* Good, because it is widely known and used in DevOps and cloud ecosystems (Kubernetes, Docker Compose, Ansible).
* Good, because it supports complex nested structures with minimal syntax (indentation-based).
* Good, because it has excellent cross-language support, useful if non-Rust tools need to parse ARI configs.
* Good, because it supports references and anchors for reducing duplication in large files.
* Neutral, because Serde integration is good (`serde_yaml` crate) but not as mature as JSON or TOML.
* Bad, because indentation-based syntax is error-prone (tabs vs spaces, invisible syntax errors).
* Bad, because YAML's type inference creates ambiguities: `yes`/`no` can be interpreted as booleans or strings depending on context.
* Bad, because YAML parsers have historically had security vulnerabilities (arbitrary code execution via `!!python/object`).
* Bad, because YAML's flexibility leads to inconsistency—multiple ways to express the same data (e.g., block vs flow style).
* Bad, because it is less common in the Rust ecosystem compared to TOML.

### Use JSON for configuration and persistence

* Good, because it is universally supported across all programming languages and platforms.
* Good, because JSON parsing is extremely fast with mature, battle-tested libraries.
* Good, because `serde_json` provides excellent Rust integration with automatic serialisation/deserialisation.
* Good, because JSON Schema can provide formal validation of configuration structure.
* Neutral, because JSON is machine-friendly but less human-friendly due to mandatory quotes and commas.
* Bad, because it lacks comments—operators cannot annotate configuration files inline.
* Bad, because JSON syntax is verbose: every key must be quoted, trailing commas are errors, no multiline strings.
* Bad, because it is less readable for humans compared to TOML or YAML, making manual editing error-prone.
* Bad, because JSON's flexibility allows inconsistent formatting (one-line vs pretty-printed), reducing readability.
* Bad, because configuration files as JSON feel unnatural in the Rust ecosystem where TOML is standard.

### Use JSON5 for improved human-friendliness

* Good, because it adds comments (`//` and `/* */`), trailing commas, unquoted keys, and multiline strings to JSON.
* Good, because it maintains JSON's structure whilst addressing readability issues.
* Good, because it parses into standard JSON, enabling compatibility with JSON tooling.
* Neutral, because `json5` crate provides Serde integration, though it is less mature than `serde_json`.
* Bad, because JSON5 is less widely known than JSON or TOML, reducing familiarity for operators.
* Bad, because tooling support is limited compared to JSON (fewer linters, formatters, editors).
* Bad, because it still retains JSON's verbosity (quotes on strings, mandatory commas except trailing).
* Bad, because adoption in the Rust ecosystem is minimal—few projects use JSON5 for configuration.

### Use INI files for simple configuration with external format for complex persistence

* Good, because INI files are simple and universally understood (`.gitconfig`, Windows `.ini`).
* Good, because they are easy to parse and edit without programming knowledge.
* Neutral, because `ini` crate provides basic parsing, but Serde integration is limited.
* Bad, because INI files lack hierarchical structure—only one level of sections, no nested objects.
* Bad, because data types are ambiguous: everything is a string, requiring manual parsing of integers/booleans.
* Bad, because INI files do not support arrays or complex structures (bootstrap peers, static routes require workarounds).
* Bad, because it does not scale: persistence snapshots (routes with metadata) cannot be represented cleanly.
* Bad, because mixed formats (INI for config, TOML/JSON for persistence) create inconsistency.

### Use environment variables for configuration and binary formats for persistence

* Good, because environment variables are universally supported in Unix/Linux environments.
* Good, because they integrate naturally with containerised deployments (Docker, Kubernetes).
* Good, because bincode persistence snapshots are compact and fast to parse (see ADR 0005).
* Neutral, because `clap` supports environment variable fallbacks for command-line arguments.
* Bad, because environment variables lack structure—flat key-value pairs require naming conventions (e.g., `ARI_IPCP_NAME`, `ARI_DIF_NAME`).
* Bad, because complex structures (bootstrap peers, static routes) are difficult to express in environment variables (requires parsing delimited strings).
* Bad, because environment variables are not inspectable after process start (no file to reference for debugging).
* Bad, because bincode snapshots are opaque—operators cannot inspect route state without custom tooling.
* Bad, because mixing environment variables (config) with bincode (persistence) creates inconsistency and operational complexity.

## More Information

### Current Implementation

TOML is used throughout ARI's configuration and persistence layer:

#### Static Configuration Files

* **Bootstrap configuration**: [config/bootstrap.toml](config/bootstrap.toml) defines IPC Process settings, Distributed IPC Facility (DIF) configuration, shim bindings, and persistence options.
* **Member configuration**: [config/member.toml](config/member.toml) includes enrolment settings with bootstrap peer addresses.
* **Structure**: Configuration files use hierarchical sections:
  * `[ipcp]`: IPC Process name, type, operational mode (bootstrap/member)
  * `[dif]`: DIF name, address allocation pool
  * `[shim]`: UDP binding address and port
  * `[enrollment]`: Timeout, retry, and backoff settings for enrolment protocol
  * `[routing]`: Static routes, persistence settings, Time-To-Live (TTL) configuration
  * `[rib]`: Resource Information Base (RIB) persistence settings, snapshot intervals, change log size

#### Dynamic State Persistence

* **Route snapshots**: [snapshots/route/ari-bootstrap.toml](snapshots/route/ari-bootstrap.toml) stores dynamic routing state with metadata.
* **Format**: Uses array-of-tables syntax for multiple route entries:

```toml
version = 1
snapshot_time = 1770386675

[[routes]]
destination = 1003
next_hop_address = "127.0.0.1:7001"
created_at = 1770386675
ttl_seconds = 3600
```

* **Versioning**: Top-level `version` field enables schema evolution for route snapshots.
* **Inspectability**: Operators can view learned routes, verify TTL expiration, and debug routing issues without custom tooling.

#### Parsing and Serialisation

* **Deserialisation**: Configuration files are parsed via `toml::from_str()` into Rust structs deriving `Deserialize` (see [config.rs](src/config.rs)).
* **Serialisation**: Route snapshots are written via `toml::to_string_pretty()` for human-readable formatting (see [routing.rs](src/routing.rs)).
* **Error handling**: Parse errors include line numbers and specific syntax issues, aiding operators in fixing malformed configs.
* **Defaults**: `#[serde(default)]` and custom default functions (`default_enrollment_timeout()`) provide sensible fallbacks for optional fields.

### Design Rationale

TOML's design aligns with ARI's operational requirements:

* **Operator-friendly**: Configuration files are intended for manual editing by system administrators, not generated programmatically.
* **Inspection during debugging**: Route snapshots must be readable during incident response—TOML enables `cat`/`grep` analysis.
* **Schema validation**: Serde's derive macros enforce structure at parse time, catching configuration errors before runtime.
* **Version control**: TOML configurations can be tracked in Git with meaningful diffs (unlike binary formats).
* **Rust ecosystem alignment**: Using TOML for configuration feels natural to Rust developers familiar with `Cargo.toml`.

### Alternatives for Specific Use Cases

Whilst TOML is chosen for configuration and route persistence, other formats are used elsewhere:

* **Binary state snapshots**: RIB snapshots use bincode (see ADR 0005) for compact encoding of large data structures not requiring human inspection.
* **Network protocol**: CDAP messages and PDUs use bincode for efficient wire-format serialisation.
* **Logging and diagnostics**: Structured logs could use JSON Lines for machine parsing (not yet implemented).

This pragmatic approach uses the right format for each purpose rather than enforcing a single format globally.

### Migration and Compatibility

TOML's versioning support enables configuration evolution:

* **Version field**: Route snapshots include explicit `version = 1` for schema versioning.
* **Optional fields**: New configuration parameters can be added as `Option<T>` with defaults, maintaining backward compatibility.
* **Validation on load**: Route snapshots validate TTL expiration during load, filtering expired routes automatically.
* **Graceful degradation**: Missing optional fields fall back to defaults without failing the parse.

Future schema changes (e.g., adding new route metadata) can increment the version field and handle multiple versions gracefully.

### Tooling and Ecosystem

TOML's adoption in the Rust ecosystem provides excellent tooling support:

* **Editors**: Syntax highlighting, validation, and auto-completion in Visual Studio Code, Vim, Emacs, and IntelliJ.
* **Formatters**: `taplo` provides automatic TOML formatting and linting.
* **Schema validation**: Third-party tools can validate TOML against schemas (though not as standardised as JSON Schema).
* **Conversion**: TOML can be converted to JSON for integration with JSON-based tooling when needed.

### Conclusion

We choose TOML for both static configuration files and dynamic route persistence in ARI. This decision prioritises human readability, type safety, and Rust ecosystem alignment whilst maintaining inspectability for operational debugging. TOML's hierarchical sections map naturally to Rust's struct-based configuration model, and its array-of-tables syntax elegantly represents collections like bootstrap peers and route entries. The format strikes an optimal balance between simplicity (avoiding YAML's pitfalls) and expressiveness (surpassing JSON's readability), making it ideal for both hand-written configuration files and machine-generated snapshots. Route persistence in TOML enables operators to inspect learned routes using standard Unix tools (`cat`, `grep`, `diff`), facilitating troubleshooting without requiring specialised debugging utilities. The decision is marked "accepted" as TOML is implemented throughout the configuration and persistence layer, with proven operational benefits in terms of debuggability and maintainability.

# myelin

A confinement and agent-loop substrate for Rust programs that run an LLM against a local
filesystem: path confinement, resource caps, a read-only tool set, endpoint/TLS plumbing
with key pinning, and the tool-calling loop. A host application links it instead of
re-implementing those parts.

> [!WARNING]
> Every line of code and documentation in this repository is LLM output, written end to end by
> Claude (Anthropic) under the direction and review of a human operator. Read and reuse it with
> that provenance in mind.

This repository is a code-review export: the sources carry no comments, and this file is
the crate's documentation. See "About this export" below.

## Modules

- **`confine`** — `ConfinedPath`: openat2-based path resolution held inside a root
  directory (`RESOLVE_BENEATH | NO_SYMLINKS | NO_XDEV | NO_MAGICLINKS`, re-applied on
  every open, plus a device pin and an eager final-component symlink refusal). The read
  tools resolve every path through it. On the tools' read path a FIFO or other
  non-regular file is refused at open (`O_NONBLOCK` + fstat) rather than blocking; the
  module's general-purpose opener is a separate case, listed under Limitations. Directory
  entry names have control bytes escaped as `<0xNN>` before they appear in any tool reply.
- **`caps`** — per-call and per-session resource limits: model iterations, tool calls,
  output bytes, and wall clock. The wall clock is per-call; the session budget counts
  the other three. When a non-wall-clock cap fires with no draft yet and at least one
  tool call already dispatched, one final tool-less wrap-up model call runs past the
  bound, limited by the inference client's request timeout.
- **`disclosure`** — one fail-closed predicate (`classify`) that decides, from two
  consumer-supplied booleans, whether a deployment is disclosure-strict and names which
  axis made it so. The axes are `sensitive` and `tier_b`; each meaning is the consumer's,
  and the two are symmetric — either one absent or true makes the deployment strict, so the
  only non-strict config is both explicitly false. It records nothing; enforcement lives in
  each consumer's startup gate.
- **`tools`** — the read-only tool set over confined paths: `read_file`, `grep`,
  `list_dir`, plus the optional `git_log` / `git_diff` and `web_search`. `grep` replies
  carry a note when a bound or skip cut the search short (entry cap, per-file byte cap,
  unreadable entries, binary skip, dot/excluded directories, deadline); `list_dir` notes
  only its own entry cap. The exceptions are listed under Limitations. Scope globs narrow
  which files the read tools surface; the dialect is globset with its defaults, so `*` and
  `?` cross `/` — `src/*.rs` matches at any depth below `src/`. A `Scope` value couples
  the compiled matcher with its source globs behind one constructor.
- **`tool_loop`** — the model/tool phase of an agent loop with the terminal seam
  abstracted: the host owns what counts as a terminal turn and how the product is
  assembled; the loop owns tool dispatch, confinement, and cap accounting. Every
  assistant `tool_calls` id receives a tool result before any further model call,
  including on cap-exit paths. `files_opened` records the confinement-normalized
  path of each file read via `read_file`. Files `grep` reads and quotes are
  attributed in its own reply lines.
- **`inference`** — backends behind one trait: an OpenAI-wire HTTP client (TCP, Unix
  socket, or mTLS with SPKI key pinning, with a per-request generation timeout) and an
  optional in-process engine adapter (feature `creatine-inprocess`) that links the
  [creatine](https://github.com/fruitvibes123/creatine) engine directly — no socket, no
  HTTP parse surface in that path.
- **`search`** — an optional web-search client (SearxNG). Query strings are
  charset-validated before dispatch and percent-encoded.
- **`endpoint` / `tls` / `config`** — endpoint parsing (TCP/UDS/TLS), rustls + ring
  with pinned keys, and the shared config surface. One agent-selection policy decides,
  from the endpoint class and the presence of a pin and client identity, which agent a
  request uses; both HTTP clients consult it for TCP and TLS endpoints. The UDS transport
  uses a fixed-socket agent outside that policy: the lmstudio client short-circuits to it
  and the search client refuses UDS. Every agent the crate builds starts from one shared
  config with ambient proxying disabled and redirects disabled.

## Git tools: trusted repository only

`git_log` / `git_diff` run the real `git` binary, resolved only from
`/usr/local/bin:/usr/bin:/bin`, with a fixed read-only argv (no shell). On a system that
keeps `git` elsewhere the tools (and their tests) fail with a spawn error rather than
consulting the ambient `PATH`.
The model controls one value (`rev_range`): charset-allowlisted, length-bounded, and
placed after `--end-of-options`. The subprocess runs with a scrubbed environment
(`GIT_CONFIG_NOSYSTEM`, `GIT_CONFIG_GLOBAL=/dev/null`) and
`--no-pager --no-textconv --no-ext-diff`, and its output is drained concurrently under a
deadline. git still reads the repository's own `.git/config`, and repo-local config can
name programs git executes; no argv flag covers every such key. Point these tools only at
a repository whose `.git/config` you trust. Under a restricted scope the git tools are
refused, since their output cannot be scope-filtered.

## Feature graph and sovereignty

Default features carry the outbound HTTP/TLS surface (`ureq` + rustls/ring). A consumer
that needs only the confinement core sets `default-features = false`; its normal
dependency graph then links neither `ureq` nor `ring`. The in-process inference adapter
is opt-in and pulls creatine's lib core only.

TLS is rustls + ring; no openssl, aws-lc, or native-tls crate is in the resolved graph.
The gate is a grep over the whole tree that must return nothing:

```
cargo tree --edges normal --prefix none --format '{p}' | sort -u | grep -Ei 'openssl|aws-lc|native-tls'
```

Note that `cargo tree -i <name>` exits non-zero with "did not match any packages" for any
name not in the graph, and single spellings miss family members (`aws-lc-rs`,
`openssl-sys`), so the grep over the full tree is the gate. `ring` appears in this
repository's own `--no-default-features` graph only as a dev-dependency of `rcgen`, the
test-certificate generator; dev-dependencies do not propagate to a consumer.

## Building and testing

Rust 1.88 or later (`rust-version` is declared; the code uses let chains). Three
configurations:

```
cargo test --locked
cargo test --locked --no-default-features
cargo test --locked --features creatine-inprocess
```

`cargo clippy --all-targets -- -D warnings` is part of the gate; the no-panic bar on the
request path is a set of deny-level clippy lints. The `creatine-inprocess` configuration
fetches the pinned creatine revision from GitHub. Three test files shell out to the `git`
binary (resolved from `/usr/local/bin:/usr/bin:/bin`, see the git-tools section) and one
to `mkfifo`. One confinement test needs two distinct filesystems among the checkout's
`target/`, the system temp dirs, and `/dev/shm`; on a single-filesystem host it fails
with a message naming `MYELIN_TEST_XFS_DIR`, which points it at a directory on a second
filesystem.

## Limitations

Known bounds of the current implementation, stated here so a reader does not have to
re-derive them:

- **Scopes with a wildcard segment are a partial oracle.** A scope is a file filter, not
  a directory-existence hedge: `list_dir` admits any directory some glob could hold a
  match under, and any scope containing a wildcard segment (`**/*.rs`, `src/*/**`, or —
  since `*` crosses `/` — `src/*.rs`) lets out-of-scope directory names and existence
  show through `list_dir`. File names and file contents stay filtered.
- **Loopback classification is by literal spelling.** Literal loopback addresses,
  including IPv4-mapped IPv6 forms, classify `Loopback`; names that merely resolve to
  loopback (DNS, `127.1`) classify `External` and then require https plus a pin — the
  strict direction. The `Loopback` locality claim depends on the crate-built agents
  (ambient proxying disabled) and, for the `localhost` spelling, on the system resolver.
- **`grep`'s deadline cannot preempt a regex call in flight.** `grep_deadline_ms` is
  checked between directory entries and between lines; a single `is_match` on one long
  line runs to completion, bounded by `max_file_bytes` and `regex_size_limit` (measured
  seconds, not minutes, in a release build; an order of magnitude more in debug).
- **The walk's `Symlink`/`Other` arm skips silently.** Symlinks are never followed. On a
  filesystem whose readdir does not report file types (`DT_UNKNOWN`), a regular file
  typed `Other` would go unsearched with no note in the reply.
- **The post-cap wrap-up call runs past the cap.** See `caps` above; it is charged
  against no cap and bounded by the inference client's request timeout.
- **File names are escaped, not rejected.** A name containing control bytes renders
  escaped in replies; the original name remains openable through its real spelling only.
- **`confine`'s general-purpose opener forwards the caller's flags.** The FIFO refusal
  above belongs to the tools' read path; a host calling the module's general-purpose
  opener directly with blocking flags can block on a FIFO until a writer appears. Callers
  own their flag choice there.

## License

myelin is licensed MIT OR Apache-2.0, at your option — see LICENSE-MIT and
LICENSE-APACHE.

The optional `creatine-inprocess` feature is off by default. Enabling it links the
[creatine](https://github.com/fruitvibes123/creatine) crate, which is licensed
GPL-2.0-or-later; the combined work is then governed by the GPL. The default graph
carries no GPL code.

## About this export

This is a published export of an internal tree. In the Rust sources, every comment — doc
comments included — is blanked in place: the comment's characters become whitespace and the
code is left untouched, so line numbers map to the internal tree and the code on each line is
byte-identical. Byte offsets are not preserved: a doc-comment line keeps its character count
and a line comment keeps its byte count, so an offset past a non-ASCII comment drifts from the
internal tree. `cargo fmt --check` reports a whitespace-only diff (`cargo fmt` rewrites it; the
suite stays green). `Cargo.toml` is scrubbed by a different rule: its comment lines are emptied
of content, so line numbers still map to the internal tree but byte offsets do not. This
README is the crate's documentation. The gates that hold on this tree are `cargo build`,
`cargo clippy`, and `cargo test` across the three configurations above.

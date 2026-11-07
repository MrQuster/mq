# MQ v2.2.0 Swarm Coordination Workflow (10 Agents)

## Architecture & Communication
- Shared integration branch: `integration`.
- Worktrees: `../mq-wt/<agent>` on branch `agent/<agent>`.
- Shared task board: `docs/BOARD.md`.
- Shared contract definitions: `docs/CONTRACTS.md`.
- File ownership matrix: `docs/OWNERSHIP.md`.

## Quality Gates Before Integration Merge
1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets --locked -- -D warnings`
3. `cargo test --workspace --locked`
4. Zero forbidden words (`rg -n "todo!|unimplemented!|TODO|FIXME|Coming soon" crates/ src/`)
5. CI green on both Ubuntu 22.04 and Windows Latest via `gh run watch`.

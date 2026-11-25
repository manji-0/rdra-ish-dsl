# RDRA-ish Agent Skills

Installable Agent Skills for modeling, reviewing, diagramming, verifying, and implementing
RDRA-ish DSL. The layout follows the [Agent Skills](https://agentskills.io/specification)
convention that both **`npx skills`** and **`gh skill`** discover:

```text
skills/
  <skill-name>/
    SKILL.md          # required (name + description frontmatter)
    references/       # optional deep docs
    samples/          # optional bundled assets (rdra-ish-verify)
```

## Install

Replace `OWNER/REPO` with this repository (`manji-0/rdra-ish-dsl`) or a fork.

### npx skills (skills.sh)

```sh
# List skills in the repo
npx skills add OWNER/REPO -l

# Install one skill (project scope)
npx skills add OWNER/REPO -s rdra-ish-verify -y

# Install all RDRA-ish skills globally
npx skills add OWNER/REPO -g --skill '*' -y

# Local checkout
npx skills add ./path/to/rdra-ish-dsl -s rdra-ish-verify -y
```

### gh skill (GitHub CLI preview)

```sh
# Validate publish layout
gh skill publish --dry-run

# Install into the current project (default agent: github-copilot → .agents/skills)
gh skill install OWNER/REPO rdra-ish-verify

# Install for Cursor / Claude / Codex, user scope
gh skill install OWNER/REPO rdra-ish-verify --agent cursor --scope user
```

## Skills

| Skill | Role |
|---|---|
| `rdra-ish-write` | Author DSL stage by stage; owns the stage table, the file layout, and the syntax |
| `rdra-ish-buc-create` | Create a BUC from requirements |
| `rdra-ish-buc-update` | Change an existing BUC |
| `rdra-ish-buc-analyze` | Answer one question: stage readiness, coverage, access, API boundaries, lifecycle findings |
| `rdra-ish-review` | Give a verdict on a model or change: a checklist and the findings format |
| `rdra-ish-diagram` | Choose and read diagram, CSV, and export views |
| `rdra-ish-verify` | Formal verification with `verify`, a native checker; TLC only as a second opinion under `--with-tlc` (+ bundled samples) |
| `rdra-ish-implement` | Implement a service from the model: `codegen`, the contracts, and `check --code` |

The skills link each other's files by relative path (`../rdra-ish-write/...`), and each fact
lives in one skill. Install them together: `rdra-ish-buc-create` and `rdra-ish-buc-update`
need `rdra-ish-write`, and the others link `rdra-ish-buc-analyze`, `rdra-ish-review`, or
`rdra-ish-verify`.

## Keeping the skills current

`python3 -I scripts/check-skill-snippets.py target/release/rdra-ish` checks every skill
against the binary: each `rdra` code block loads with `check`, each `$ rdra-ish ...`
transcript still prints what the skill quotes, and each command, flag, and flag value
exists. CI runs it in the `check` job. Its header describes the markers it reads.

## Formal-verification samples

Canonical `.rdra` examples ship inside `rdra-ish-verify/samples/` so they stay available
after a skill install. In this monorepo, `samples/formal-verification/` (and
`samples/formal-verification-fail/`) are symlinks to those files for CLI tests and docs.

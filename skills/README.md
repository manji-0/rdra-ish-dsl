# RDRA-ish Agent Skills

Installable Agent Skills for modeling, reviewing, diagramming, and formally
verifying RDRA-ish DSL. Layout follows the
[Agent Skills](https://agentskills.io/specification) convention discovered by
both **`npx skills`** and **`gh skill`**:

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
| `rdra-ish-write` | Author DSL by refinement stage (step files 0 to 6) |
| `rdra-ish-buc-create` | Create a BUC from requirements |
| `rdra-ish-buc-update` | Extend an existing BUC |
| `rdra-ish-buc-analyze` | Analyze coverage, access, API boundaries, and lifecycle findings |
| `rdra-ish-diagram` | Choose diagram / CSV / export views |
| `rdra-ish-review` | `check` / `lint`-oriented model review checklist |
| `rdra-ish-verify` | TLA+/TLC formal verification with `verify` (+ bundled samples) |

## Keeping the skills current

`python3 -I scripts/check-skill-snippets.py target/release/rdra-ish` checks every skill
against the binary: each `rdra` code block loads with `check`, each `$ rdra-ish ...`
transcript still prints what the skill quotes, and each command, flag, and flag value
exists. CI runs it in the `check` job. Its header describes the markers it reads.

## Formal-verification samples

Canonical `.rdra` examples ship inside `rdra-ish-verify/samples/` so they remain
available after skill install. In this monorepo,
`samples/formal-verification/` (and `samples/formal-verification-fail/`) are
symlinks to those files for CLI tests and docs.

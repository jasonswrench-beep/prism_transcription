# Standing rules for this repository

These hold for every session and every commit. They override any default attribution behaviour.

## Commits

1. **Identity.** Commit as `jasonswrench-beep <324576087+jasonswrench-beep@users.noreply.github.com>`. It is set in
   this repository's own git config (`git config --local user.name` / `user.email`). Check it before every commit:

   ```
   git config --local --get user.name
   git config --local --get user.email
   ```

   If either is missing or different, set it again before committing.

2. **No AI attribution, ever.** No `Co-Authored-By:` lines and no "Generated with Claude Code" lines in commit
   messages, tag messages, pull request descriptions or release notes.

## Releases

3. **Commit and tag before any installer build.** Before building an installer, commit everything (the working tree
   must be clean: `git status --porcelain` prints nothing), then tag that commit. Build from the tagged commit, so each
   release tag points at the exact commit the build came from. Never move a release tag to another commit; a
   rebuilt release gets a new tag.

## Also standing (from the specification and Jason)

- The README never describes the app as an alternative to, or compares it with, a named commercial product.
- Ask before pushing to GitHub.
- No access token (Hugging Face or any other) is ever written to a file, committed or shipped.
- What only a person can check goes in `TESTING-TODO.md`.

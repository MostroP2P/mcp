# Fixtures

`outcomes.json` holds documents printed by `mostro-cli --output json`, keyed by a name, each with the exit code and the document:

- copied from the `contracts/examples.json` files of the CLI repository (`specs/001` to `specs/011`, at tag `v0.17.0`; the eleven documents first taken at `develop` `a364055` are byte-identical there), which that repository
  keeps byte-identical to what its constructors build;
- `schema`: the `schema` command's outcome reduced to the three fields the contract check reads (`name`, `cli_path`, `output_modes`) of every command in
  the output of `mostro-cli --output json schema` at `v0.17.0`.

Refresh them with `refresh.py` when the CLI contract changes; the unit and stdio tests fail if a document no longer parses or maps.

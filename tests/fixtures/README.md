# Fixtures

`outcomes.json` holds documents printed by `mostro-cli --output json`, keyed by a name, each with the exit code and the document:

- copied from the `contracts/examples.json` files of the CLI repository (`specs/001` to `specs/008`, at `develop` `a364055`), which that repository
  keeps byte-identical to what its constructors build;
- `schema`: the `schema` command's outcome reduced to the three fields the contract check reads (`name`, `cli_path`, `output_modes`) of every command in
  `docs/schemas/discovery-v1.json`.

Refresh them from the CLI repository when its contract changes; the unit and stdio tests fail if a document no longer parses or maps.

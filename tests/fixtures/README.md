# Fixtures

`outcomes.json` holds documents printed by `mostro-cli --output json`, keyed by a name, each with the exit code and the document:

- copied from the `contracts/examples.json` files of the CLI repository (`specs/001` to `specs/008`, at `develop` `a364055`), which that repository
  keeps byte-identical to what its constructors build;

Refresh them from the CLI repository when its contract changes; the unit and stdio tests fail if a document no longer parses or maps.

# External first-party declarations

Generated test data from <https://github.com/korri-os/plugins>. `REVISION` is
its full Git commit. `SHA256SUMS` records the exact producer bytes at
`plugins/{fex,proton,steam}/plugin.ts`. These are not production plugins.
The publisher alone owns their source, native recipes, scripts and unit tests.

Refresh from an immutable producer commit on a build host:

```sh
python3 services/korrid/tests/update-first-party-fixtures.py PUBLISHER_CHECKOUT FULL_COMMIT_SHA
```

Commit the generated files together. Do not edit them by hand. The updater
reads `git show`, not working-tree files. `korrid-check` and the strict
first-party typecheck verify their checksums. Typechecking copies the exact
bytes into their original `plugins/<name>` layout beside the newly generated
Core treaty. Core retains real registry, discovery, launch, composition and
admission tests; it does not run publisher-owned plugin unit tests.

A checked-in snapshot avoids a cyclic normal flake input: the publisher already
consumes Core. It also lets Cargo tests run offline without hydrating a source
cache. The cost is an explicit refresh when the producer changes. This is not a
claim about the latest published packages; Core's published-package consumer
checks remain a separate gate.

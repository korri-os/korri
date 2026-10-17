# Plugins declare the games they prepare; discovery never scans plugin data

A plugin can declare a game directly, with the files it prepared, as it declares systems and runners. Discovery never walks a plugin's own data, so build trees and prepared files are never mistaken for content. When the original content of a declared game is also found in a source, the declaration names that content's hashes and both join into one game, playable through either route.

Before this, OpenGOAL's prepared Jak files reached the catalog only because the scanner walked `users/default`, and that same walk cataloged 2,290 compiler object files.

## Considered Options

- Plugins declare a source over their own data and discovery scans it: one mechanism, but it walks build output again.
- Prepared files are never games, only the original content is: loses games whose original content is not in any source.

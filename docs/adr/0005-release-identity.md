# A release is identified by its content bytes, not by its file

A file release's identity is the SHA-256 of its content; the same read also computes SHA-1, because published hash lists (No-Intro, Redump) carry SHA-1. "Content" excludes what is not the game:

- A cartridge header is excluded by a header rule the system declares, as No-Intro and clrmamepro's header detectors do, so headered and headerless dumps are one release.
- A bin/cue disc is identified by its ordered track hashes; the `.cue` and `.m3u` descriptors are not hashed, as in Redump.
- A CHD is identified by the raw-data SHA-1 stored in its header, which survives recompression, so a CHD is identified without reading the whole disc.
- A multi-disc game's discs form one release, because a launch needs all of them.

Archives are not supported in the first version.

## Consequences

A CHD and a bin/cue of the same disc are different releases of the same game. They join only through a hash list or a link. This was chosen to avoid decompressing every CHD on a handheld.

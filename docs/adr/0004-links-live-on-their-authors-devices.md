# Links live on their authors' devices and peers merge them

A link is signed with its author's person key and kept on the devices where it was made. Peers pull links when they read the catalog and merge them as a set, so a link reaches every device without a central server or a push channel. A peer that is offline sees new links when it next reads. Only the author's signature can undo a link; two people's contradicting links go to the attention list.

Nothing in korrid replicated person data before this: the only shared federation document is device membership (`federation/peers.json`), and play history is local to each device.

## Considered Options

- Push every link to every device when it is made: everyone sees it at once, but needs delivery, retries and ordering.

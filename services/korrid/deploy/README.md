# korrid deployment

The standalone Zao systemd user-service deployment was removed with the
clean-cut local signer. A user service cannot give the signer a distinct UID or
make its private state inaccessible to korrid.

Deploy Linux devices through `services/korrid/nixos-module.nix`, normally via
the composed Linux host module. That module starts `korri-local-signer.service`
and `korri-local-signer.socket` under their dedicated identity before
`korrid.service`. The local signer keeps nothing on disk: it holds a person
key in memory only while the person uses the recovery words (ADR 0082).

The remaining files in this directory are historical Zao data and one-off
provisioning or fixture inputs. They are not a korrid launch path.

## One-off migrations

`migrate-plugin-artwork.py` moves a device's artwork state when the korrid
without the built-in SteamGridDB fetcher is deployed (ADR 0032, section 5).
Run it once on each device, with korrid stopped, against korrid's private
state root. Its docstring gives the steps and the check afterwards.
`test-migrate-plugin-artwork.py` is its test.

`migrate-measured-media.py` moves a device's stored art from the fixed
roles to kinds when the korrid of ADR 0091 is deployed: each record becomes
`<playable id>:<plugin id>:<kind>:<n>`, and each art plugin's old
`attempts.json` is set aside so that korrid asks the plugin again. Run it
once on each device, with korrid stopped, against korrid's private state
root. Its docstring gives the steps and the check afterwards.
`test-migrate-measured-media.py` is its test.

`person-key-move.md` moves a device whose local signer kept a person key in
`/var/lib/korri-local-signer` to the person key of its recovery words (ADR
0082). Do it once on each such device after the change is deployed.

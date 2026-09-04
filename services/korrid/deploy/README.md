# korrid identity rollout

The checked `upstreams.android.json` file is a public template. It does not contain a device key. `push-zao.sh` reads Zao's public key from `korrid identity status` after the service health check. It writes the final secure Android peer file to `.tmp/upstreams.android.json`. Set `ZAO_KORRID_URL` when Zao does not use the standalone deployment default `http://zao:43117`. For example, the Mountainous service uses `ZAO_KORRID_URL=http://zao:39217`.

Set `KORRID_RELAYS_JSON` to a JSON array of one to eight `wss://` relay URLs before deployment. The rollout has no relay default. The generated systemd environment file contains only public relay URLs.

## Protocol cut and rollback

Deploy the Android peer caller and the Zao peer receiver together. The new caller sends signed and NIP-44-encrypted requests to `/peer-rpc`. The new receiver has no plaintext `/rpc` fallback on its LAN listener.

Roll back both ends together. An old plaintext caller cannot use the new receiver. A new secure caller cannot use the old plaintext receiver. Rolling back only one end makes peer catalog and launch calls unavailable.

Identity state is not package state. Package rollback must not replace or delete `$HOME/.local/state/korrid/private/identity`. Use `korrid identity reset` only for an explicit device reset. Reset creates a new device public key, so regenerate `.tmp/upstreams.android.json` and redeploy every peer that names the old key.

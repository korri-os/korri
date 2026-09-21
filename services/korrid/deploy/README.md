# korrid deployment

The standalone Zao systemd user-service deployment was removed with the
clean-cut local signer. A user service cannot give the signer a distinct UID or
make its private state inaccessible to korrid.

Deploy Linux devices through `services/korrid/nixos-module.nix`, normally via
the composed Linux host module. That module starts `korri-local-signer.service`
and `korri-local-signer.socket` under their dedicated identity before
`korrid.service`.

The remaining files in this directory are historical Zao data and one-off
provisioning or fixture inputs. They are not a korrid launch path.

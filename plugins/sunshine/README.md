# Sunshine streaming host plugin

`@korri:sunshine` is the removable Linux streaming-host integration. The package ships three native systemd units:

- `korri-sunshine.service` runs the reviewed Sunshine build as `korri`.
- `korri-sunshine-certificate-control.socket` gives korrid the private certificate-trust channel.
- `korri-sunshine-input-setup.service` owns the temporary group and runtime UHID permission rule. The core `korri-input-seat-receiver.service` owns persistent shared seats independently of this plugin.

The plugin requests KMS capture, broad device visibility, `CAP_SETPCAP`, and `CAP_SYS_ADMIN`. Approval binds those requests, both root helper commands, the named udev rule artifact, every native unit, and the exact package closure. The plugin host starts all three units before it opens the declared Sunshine ports. It never opens the administrative TCP port `47990`.

Removal stops all three plugin units, checks native cleanup, removes the runtime UHID rule and temporary group, closes the firewall rules, and releases the package selection. It does not stop the core receiver or change persistent seat permissions. It preserves `/home/korri/.config/sunshine`, including Sunshine identity, settings, and paired-client state. Korrid remains the only owner of the bounded stream-client trust effect.

The plugin uses globally unique systemd names. It does not replace a host `sunshine.service`, add a NixOS module, or keep a disabled host unit in the image.

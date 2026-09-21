# Current streaming-host unit fixture

These three files are the NixOS-rendered units from
`services/inputd/nix/korri-linux-host.nix` at this commit. The fixture uses the
RG353M configuration with `services.korriLinuxHost.sunshine.inputSeats.enable`
set to `true`, so all three current units are present.

Regenerate each file from `c.config.systemd.units.<name>.text`, where:

```nix
let
  f = builtins.getFlake (toString ./.);
  c = f.nixosConfigurations.rg353m.extendModules {
    modules = [
      { services.korriLinuxHost.sunshine.inputSeats.enable = true; }
    ];
  };
in c
```

The test consumes the rendered files unchanged. It does not restate their
systemd configuration in TypeScript or in a Korri-specific schema.

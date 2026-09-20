# Compose the product module and migrate RG353M

Status: ready-for-agent
Blocked by: None

## What to build

Make RG353M install the same complete Korri product that every device will use. Put required product behavior in one product module. Leave only RG353M hardware facts and recorded limits in its device configuration.

## Acceptance criteria

- [ ] One product module composes base policy, the Linux host, the shared portal browser path, the plugin host, device-cache policy, and the product runtime account.
- [ ] The product account is `korri`, uid 1000, group `korri`, gid 1000, with home `/home/korri`.
- [ ] RG353M imports the product module and no longer declares, removes, swaps, or force-disables product services in its device module.
- [ ] RG353M supplies only hardware facts and explicit recorded limits. Product-owned browser focus behavior and account values are not device options.
- [ ] The user approves whether the existing `relays`, `surfaceId`, and korrid bind-address values are hardware facts or product constants before the implementation classifies them.
- [ ] A product check enumerates exported `nixosConfigurations`, rejects a configuration that does not install the product module or a required part, checks the fixed runtime account, rejects force-disabled product units, and contains no device opt-out list.
- [ ] RG35XXSP is temporarily absent from `nixosConfigurations` on the integration branch until ticket 18. It remains a product device.
- [ ] The integration branch is not merged to `main` until tickets 08 through 11 make the product check pass for every device that remains exported.
- [ ] Focused Nix evaluation tests cover the module and the product check. Passing the check is not recorded as hardware support.
- [ ] No account migration, alias, fallback read, dual write, compatibility branch, or runtime migration is added.

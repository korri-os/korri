# Device sleep state

The approved Nix declaration is `services.korriProduct.sleep.states`. Its only state names are `light sleep`, `deep sleep`, and `hibernation`. A device lists every state that works on its kernel. An empty list records no sleep support.

| Device | Declared states | Recorded limit |
|---|---|---|
| RG353M | None | No suspend or light-sleep path has been verified. |
| RG DS | None | No suspend or light-sleep path has been verified. |
| R36T Max | None | No suspend or light-sleep path has been verified. |
| RP Mini V2 | None | OLED idle blanking is not sleep; suspend is unverified. |
| Odin 2 Portal | None | Mainline suspend and wake are unverified. |
| RG35XXSP | None | No suspend or light-sleep path has been verified. |

While no state is declared, logind shuts down cleanly on a power press or lid close. The Odin no longer ignores these controls. A screen-off idle timer does not make a device sleep.

The owner approved a 900-second product-wide light-sleep shutdown delay. This value has no runtime effect today. Do not declare light or deep sleep before the exact-session freeze, wake, and screen handlers exist. Hibernation remains unimplemented. No device has passed physical suspend acceptance.

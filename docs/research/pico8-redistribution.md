# PICO-8 games with redistribution permission

## Answer

Yes. The original developer pages below explicitly permit redistribution. Three released games restrict redistribution to noncommercial purposes. One small experimental game has an explicit MIT declaration covering original code and assets.

This note verifies published permission, not legal clearance for a particular Korri business model. No game was installed or played. Runtime compatibility, third-party rights, and final packaging remain unchecked.

## Verified developer declarations

| Game | Primary evidence | Permission and cost |
|---|---|---|
| Celeste Classic 2: Lani's Trek | [Noel Berry's cartridge post](https://www.lexaloffle.com/bbs/?tid=41282) labels `celeste_classic_2-5` as `License: CC4-BY-NC-SA`. It credits Maddy Thorson, Noel Berry, and Lena Raine. | CC BY-NC-SA 4.0 permits noncommercial redistribution with attribution and license information. Adaptations must meet ShareAlike. Separate permission is needed for commercial use. |
| Mot's Grand Prix | [Tom Mulgrew's itch.io page](https://tommulgrew.itch.io/mots-grand-prix) explicitly lists MIT for code and CC BY-NC-SA 4.0 for assets. [His BBS cartridge](https://www.lexaloffle.com/bbs/?tid=44700) labels `mot_grandprix2-0` as CC4-BY-NC-SA. | Complete-game redistribution remains noncommercial-only because of the assets. The itch.io PICO-8 release uses multiple cartridges. Preserve the full set and test the chosen runtime. |
| Mot's 8-Ball Pool | [Tom Mulgrew's itch.io page](https://tommulgrew.itch.io/mots-8-ball-pool) explicitly lists MIT for code and CC BY-NC 4.0 for assets. It offers `mot_pool.p8`. | Complete-game redistribution is noncommercial-only, with attribution and license information. Unlike Grand Prix's asset license, this asset license has no ShareAlike requirement. |
| Lander test | [whippet-code's repository README](https://github.com/whippet-code/pico-8) states: "Unless otherwise stated, all original code and assets in this repository are released under the MIT License." [The actual cartridge](https://raw.githubusercontent.com/whippet-code/pico-8/main/lander.p8) identifies itself as `lander test`, credits `m ivkovic (whippet)`, and contains Lua, graphics, and sound. No conflicting license notice was found in that cartridge. | The published MIT declaration covers original code and assets and permits commercial redistribution. Preserve the copyright and MIT permission notice. This is an experiment, not a verified production-quality game. The repository does not supply a separate MIT LICENSE file with a completed copyright notice; resolve packaging of that notice before shipping. |

## License obligations

The actual [CC BY-NC-SA 4.0 legal code](https://creativecommons.org/licenses/by-nc-sa/4.0/legalcode.en), section 2(a)(1), grants the right to "reproduce and Share the Licensed Material, in whole or in part, for NonCommercial purposes only". Section 3 requires attribution, supplied notices, a license link or text, and identification of modifications. Section 3(b) applies ShareAlike to adaptations, not automatically to unrelated Korri code.

The [CC BY-NC 4.0 legal code](https://creativecommons.org/licenses/by-nc/4.0/legalcode.en) has the same noncommercial restriction and attribution duties, without the ShareAlike condition.

Both licenses define NonCommercial as "not primarily intended for or directed towards commercial advantage or monetary compensation". A free image is not automatically a noncommercial use. Bundling on sold devices or as part of commercial promotion needs separate permission or legal review.

The [MIT text](https://opensource.org/license/mit) grants distribution and sale rights, subject to retaining the copyright and permission notice. A MIT code license alone does not license graphics or music. For each selected release, retain its credits and exact permission evidence alongside its artifacts. These licenses do not warrant that the author controls every third-party right.

## Candidates not cleared by this pass

| Candidate | Why it is not cleared |
|---|---|
| Original Celeste Classic | [NoelFB/Celeste's README](https://raw.githubusercontent.com/NoelFB/Celeste/master/README.md) limits MIT to repository code and excludes commercial game assets. [The PICO-8 directory README](https://raw.githubusercontent.com/NoelFB/Celeste/master/Source/PICO-8/Readme.md) describes a C# port, not a licensed original Lua cartridge. Do not use this repository's MIT license to justify bundling the original cartridge. |
| Porklike | [The original BBS post](https://www.lexaloffle.com/bbs/?tid=37045) marks `porklike-2` as `No License` and credits third-party music and art permissions. Free download and tutorial source are not redistribution permission. |
| Villager | [The original itch.io page](https://partnano.itch.io/villager) lists MIT for code but no asset license and credits inspiration from Cluly's tileset. This page alone does not clear the full cartridge. |
| Marc Duiker's games | [The developer repository](https://github.com/marcduiker/pico-8-games) has an MIT license. This pass did not match individual released games to licensed cartridge files or examine asset credits. |

## Runtime is separate

A cartridge license does not grant permission to redistribute the full official PICO-8 application. This pass did not obtain its actual application license, so it does not clear that application for bundling.

[Lexaloffle's official FAQ](https://www.lexaloffle.com/pico-8.php?page=faq) confirms HTML5 and native exports exist and permits selling one's exported cartridges with contributing authors' permission. Do not confuse an exported game runtime with the full PICO-8 editor/player application.

[FAKE-08's developer README](https://raw.githubusercontent.com/jtothebell/fake-08/master/README.md) identifies an MIT license with additional dependency licenses and warns that compatibility is incomplete. An open runtime is a possible separate route, not proof that these games work on Korri. No runtime was selected, built, or deployed.

## Recommendation

For an unrestricted Korri image, prefer a full-game MIT, CC0, or CC BY grant, or obtain written permission covering code, art, music, redistribution, and the intended commercial use. This narrows the catalogue and takes more author contact, but avoids a noncommercial limit on the shipped games.

Celeste Classic 2 and the two Mot games are concrete candidates for a genuinely noncommercial bundle. If Korri will be commercial, request separate permission before including them. No release bundle was changed by this research.

# Exact immutable publisher outputs, not recipes evaluated with Core.
# Grounding: build-9514b1b382d3, build-9514b1b382d3-solarus, build-ac8fae593e42-zquest, build-ac8fae593e42
# and build-75eb41b75907 (Starter pack with package-file releases)
# and build-824050bfa4c3 (PICO-8, Homebrew Hub and RetroBrews acquisition plugins)
# and build-365ce4e054d5 (ADR 0012 FEX, Proton and Steam)
# and build-30594ae66c81 (save sync RetroArch save locations, Core plugin-run fixtures)
# and build-adaccd5610ce (Sunshine guarded KMS FIFO and bounded vblank waits)
# and build-bd4bfcfe4e9a / build-bd4bfcfe4e9a-starter-pack (FAKE-08 save states)
# and build-e40a4300533b-stream-play (Moonlight and Sunshine stream play)
# paths-SYSTEM.txt and revision.txt from their immutable releases;
# IDs below come from the current host seed report over each published package.
# Plugin updates change these pins and matching offline proof assets explicitly.
{ system }:
let
  published = {
    aarch64-linux = {
      korri-plugin-beetle-ngp = "/nix/store/3xrfprf5ap24yxcg8fvm7y5cs5i0m5gv-korri-plugin";
      korri-plugin-beetle-pce-fast = "/nix/store/s75363kxf27ylvndz6qy5slqpfi4w53z-korri-plugin";
      korri-plugin-beetle-wswan = "/nix/store/c4fwnr3ci45ilm90g1lsrs9bjz1n2ksx-korri-plugin";
      korri-plugin-fake08 = "/nix/store/yy6pl7m6k7c2icgvmjig1xp5dcki83wg-korri-plugin";
      korri-plugin-fbneo = "/nix/store/ja9lxp2qbrlgdg49xi1017gdc0dcnjx8-korri-plugin";
      korri-plugin-fex = "/nix/store/gishs8dfwq5f16zs0qck459vmvpjx421-korri-plugin";
      korri-plugin-flycast = "/nix/store/r65vxzr72zg24ysmysyl3l732wyy3fpw-korri-plugin";
      korri-plugin-gambatte = "/nix/store/qp13n81bsy8yk6jrvz0ps43171pdwfsm-korri-plugin";
      korri-plugin-gamescope = "/nix/store/6y3g0yv13pmsq30a0fbigk40570h94lh-korri-plugin";
      korri-plugin-genesis-plus-gx = "/nix/store/q4zdkb3c4gyr5x3l85jfqpz8900zgnb8-korri-plugin";
      korri-plugin-gw = "/nix/store/wwsbzcymki4sx9qmkbmpdl8lrh8gi62w-korri-plugin";
      korri-plugin-handy = "/nix/store/4xvkgf6mz0fm6myrww4z3nz7gjackfry-korri-plugin";
      korri-plugin-homebrewhub = "/nix/store/3wwimpj0mmmr1qr6zldjyxdp81jfc45s-korri-plugin";
      korri-plugin-melonds = "/nix/store/c0d2jvqmcvw7sjdp9n6v20h4kzrbczq7-korri-plugin";
      korri-plugin-mgba = "/nix/store/92c82hj672is3k2m72yvvihjr2jwwn4p-korri-plugin";
      korri-plugin-moonlight = "/nix/store/7m2xzp52x5f9qamqyqbzx3052268knlv-korri-plugin";
      korri-plugin-mupen64plus = "/nix/store/id4grwngdkplgfwl5maxv5rx5ijjm8p6-korri-plugin";
      korri-plugin-nestopia = "/nix/store/3wvfjkc89b5pyskzyqv57k58v12kl30i-korri-plugin";
      korri-plugin-pcsx-rearmed = "/nix/store/rn3w0wg1cr9s5x13sdfakygrv22f7p00-korri-plugin";
      korri-plugin-pico8 = "/nix/store/7yqqd99p7636bi39bgzgr5zlkgblxzvx-korri-plugin";
      korri-plugin-picodrive = "/nix/store/z295f88bcwbfjqq44jqb3ql7r638sb5m-korri-plugin";
      korri-plugin-prosystem = "/nix/store/dajl085b7ykfszq6gs78xjlk6rn3wklh-korri-plugin";
      korri-plugin-proton = "/nix/store/2qspr627wbdprz9cg81143gn2kqyrh14-korri-plugin";
      korri-plugin-retroarch = "/nix/store/ns8rn3fghqakh3lb23v8xp9m9917kwq0-korri-plugin";
      korri-plugin-retrobrews = "/nix/store/zg4s6g5ad115lfzpq3g3pfil6v4ml1zn-korri-plugin";
      korri-plugin-snes9x = "/nix/store/2qwkadpsdmicv6jhp3gad2zc8w7iincv-korri-plugin";
      korri-plugin-solarus = "/nix/store/vpirf6d752ax7mbilbx8wv2g61d3vl5r-korri-plugin";
      korri-plugin-ssh = "/nix/store/7frsp72c18bp69vjr7avs0l4lmjankb5-korri-plugin";
      korri-plugin-starter-pack = "/nix/store/9rz2xjv1ppqzkgxkivvkqdy40573z4kj-korri-plugin";
      korri-plugin-steam = "/nix/store/7ks62xvwfl172rl7i4z8fmn8iiyngaaq-korri-plugin";
      korri-plugin-stella = "/nix/store/xxa3zwd5kdvxankjx38b0rnf0acl64aj-korri-plugin";
      korri-plugin-sunshine = "/nix/store/mb63wipdx51lvl0afbkpwyjrn280fzb5-korri-plugin";
      korri-plugin-zquest-classic = "/nix/store/fgpg2glx2n202j975w62d73jlj5l86xx-korri-plugin";
    };
    x86_64-linux = {
      korri-plugin-beetle-ngp = "/nix/store/5j46dw20asxk5d4b68vkiv3adwpl6aqi-korri-plugin";
      korri-plugin-beetle-pce-fast = "/nix/store/sh7ws8z6864hbb88gzqbqszmq65rgs4m-korri-plugin";
      korri-plugin-beetle-wswan = "/nix/store/ijmhx8n8aipwf8g72ajzllqwkhzl2pq7-korri-plugin";
      korri-plugin-fake08 = "/nix/store/10sl0wdsd6m7pnrsiq1wzm5vzm6x4f7f-korri-plugin";
      korri-plugin-fbneo = "/nix/store/awm22whsm51820clsiwg5z2hd1zf6cys-korri-plugin";
      korri-plugin-fex = "/nix/store/w0nzpwryn6ckkfrrwka0lbm5iksm3y15-korri-plugin";
      korri-plugin-flycast = "/nix/store/6k461lhhjz1bmcc7r6w3qyrb401awa2i-korri-plugin";
      korri-plugin-gambatte = "/nix/store/f5pvfwbqh306avykwwcghjybk0kzl9fs-korri-plugin";
      korri-plugin-gamescope = "/nix/store/b6c5bsvk0divymqlgfjbvpqkkim8hacr-korri-plugin";
      korri-plugin-genesis-plus-gx = "/nix/store/5fcr2jg9yn1qczd7zzgsaacy744agqc9-korri-plugin";
      korri-plugin-gw = "/nix/store/i2gfjahcrqz92rxr32884h658ghp0ylq-korri-plugin";
      korri-plugin-handy = "/nix/store/j0q218i718ja2llkabdlha51hagp0h02-korri-plugin";
      korri-plugin-homebrewhub = "/nix/store/fdk5i6vnxbwmq0a1jx7kpp64zi33fx08-korri-plugin";
      korri-plugin-melonds = "/nix/store/3v8kkkf9hcdq21hazqqnb3903kr2ajc5-korri-plugin";
      korri-plugin-mgba = "/nix/store/0nhcvywwj0n2i6wnwsb2kwagfppqh1xa-korri-plugin";
      korri-plugin-moonlight = "/nix/store/6fmj0jsplvp8q1w4vb2xsig28yk9lvm4-korri-plugin";
      korri-plugin-mupen64plus = "/nix/store/8njafx3sxgxvidy58g5xx2f6mfk1mg8f-korri-plugin";
      korri-plugin-nestopia = "/nix/store/38fw1p9cc32xqlkh48x8d402dvamvyi9-korri-plugin";
      korri-plugin-pcsx-rearmed = "/nix/store/abh7mbp48wmwim70mfh2xrj9r58ck60s-korri-plugin";
      korri-plugin-pico8 = "/nix/store/ymsxs9wj428qldpchpmrz3wixjgkfkk4-korri-plugin";
      korri-plugin-picodrive = "/nix/store/4hjgyvxczh8ysi8aa346b8kh5kp7a124-korri-plugin";
      korri-plugin-prosystem = "/nix/store/qbfy52zzfrlgc2vg6mq1magcg745fxhs-korri-plugin";
      korri-plugin-proton = "/nix/store/rjan002x2hvpk9n0ncwzxvzfvr25ri6v-korri-plugin";
      korri-plugin-retroarch = "/nix/store/wl0ypr4wvr08ksr9f3iwa1bw8pykp3m8-korri-plugin";
      korri-plugin-retrobrews = "/nix/store/mkfap2znlvqg5ralffl3admjkv1mp38k-korri-plugin";
      korri-plugin-snes9x = "/nix/store/sjcxj17p6r5jz0pa8xy9jd1fjap8vmc3-korri-plugin";
      korri-plugin-solarus = "/nix/store/51rh3jqhlba9qvpkl7j5ba7mlwfggp8x-korri-plugin";
      korri-plugin-ssh = "/nix/store/l81yra5g4122wm8cpv2pdpgqjw5ws5p1-korri-plugin";
      korri-plugin-starter-pack = "/nix/store/fqpjdgvs9jdmk4idjhfh5zrr8n047q3j-korri-plugin";
      korri-plugin-steam = "/nix/store/3k5pfp2c1di8rkav0dr6r7j79fqfp98d-korri-plugin";
      korri-plugin-stella = "/nix/store/0qhlw9pl972prd78d7qcdfjfhc55r2bm-korri-plugin";
      korri-plugin-sunshine = "/nix/store/qvsa4hm0v0c3mfxj34ab1isff9mb6f6b-korri-plugin";
      korri-plugin-zquest-classic = "/nix/store/p9cgjc5c7hw8vry05arnw5dhcxgcwd06-korri-plugin";
    };
  };
in
builtins.mapAttrs (
  name: path:
  builtins.appendContext path {
    "${path}" = {
      path = true;
    };
  }
) published.${system}

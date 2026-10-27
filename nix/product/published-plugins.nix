# Exact immutable publisher outputs, not recipes evaluated with Core.
# Grounding: build-9514b1b382d3, build-9514b1b382d3-solarus, build-ac8fae593e42-zquest, build-ac8fae593e42
# and build-75eb41b75907 (Starter pack with package-file releases)
# and build-824050bfa4c3 (PICO-8, Homebrew Hub and RetroBrews acquisition plugins)
# and build-365ce4e054d5 (ADR 0012 FEX, Proton and Steam)
# and build-30594ae66c81 (save sync RetroArch save locations, Core plugin-run fixtures)
# and build-adaccd5610ce (Sunshine guarded KMS FIFO and bounded vblank waits)
# and build-bd4bfcfe4e9a / build-bd4bfcfe4e9a-starter-pack (FAKE-08 save states)
# and build-e40a4300533b-stream-play (Moonlight and Sunshine stream play)
# and build-57de061520cc-settings-vocabulary (nested settings vocabulary, ADR 0037)
# and build-b65ce7b266b4-moonlight-v4l2request (Moonlight stateless V4L2 Request decode)
# and build-dbb0917a5b58-moonlight-h700-gl (Moonlight zero-copy GL presentation for the H700)
# paths-SYSTEM.txt and revision.txt from their immutable releases;
# IDs below come from the current host seed report over each published package.
# Plugin updates change these pins and matching offline proof assets explicitly.
{ system }:
let
  published = {
    aarch64-linux = {
      korri-plugin-beetle-ngp = "/nix/store/7isiaqzb4gxfq1864rp95ic5dls2933w-korri-plugin";
      korri-plugin-beetle-pce-fast = "/nix/store/xk14hjhrmp4m91dwaslm2qw9hd9awq8d-korri-plugin";
      korri-plugin-beetle-wswan = "/nix/store/nz2b0c3bdpriwibfrwjm6c177zgqicb8-korri-plugin";
      korri-plugin-fake08 = "/nix/store/xv1l7xyc7bpv39m177wc4s0xgvr6193k-korri-plugin";
      korri-plugin-fbneo = "/nix/store/xpi1wr9ivlwmcxaszlz8zj1rzxk4ipld-korri-plugin";
      korri-plugin-fex = "/nix/store/gishs8dfwq5f16zs0qck459vmvpjx421-korri-plugin";
      korri-plugin-flycast = "/nix/store/vzw4hym3qyjmpw1dkjc0n27hzb3rkwxv-korri-plugin";
      korri-plugin-gambatte = "/nix/store/7xdmbn10pgaq6p41jsdw1q1s2drpwd18-korri-plugin";
      korri-plugin-gamescope = "/nix/store/m0130v2m4kqrs4sfg89pjygnbmg2k24b-korri-plugin";
      korri-plugin-genesis-plus-gx = "/nix/store/lpzvk0sx6bqcy780sqdjc73qjlf0p3cn-korri-plugin";
      korri-plugin-gw = "/nix/store/7fvcf92c43smxiwqk6bl65jjh66d4n8b-korri-plugin";
      korri-plugin-handy = "/nix/store/0jw1kr2m6gnl0lk3ky54g89jlxhyj2n2-korri-plugin";
      korri-plugin-homebrewhub = "/nix/store/3wwimpj0mmmr1qr6zldjyxdp81jfc45s-korri-plugin";
      korri-plugin-melonds = "/nix/store/pixmh9gch884ibrqhgpq4ck8hk931bd9-korri-plugin";
      korri-plugin-mgba = "/nix/store/j3hs59kazd336wdr7qsfyj7q7ga34bnd-korri-plugin";
      korri-plugin-moonlight = "/nix/store/84cqgpjixgcf1a96g6gpgqvvpr475xfg-korri-plugin";
      korri-plugin-mupen64plus = "/nix/store/adx8r2y8677yqcrkm0x15f0k1l3yg94v-korri-plugin";
      korri-plugin-nestopia = "/nix/store/7bfn8ir63qnyg89scpyj5wgvf1h73zil-korri-plugin";
      korri-plugin-pcsx-rearmed = "/nix/store/678d4lp9v6ldzmix8q7836pln6xpfafn-korri-plugin";
      korri-plugin-pico8 = "/nix/store/7yqqd99p7636bi39bgzgr5zlkgblxzvx-korri-plugin";
      korri-plugin-picodrive = "/nix/store/dd8yr20axvi78l3g3m3969a5vivk88jr-korri-plugin";
      korri-plugin-prosystem = "/nix/store/g70zbymvivgjvcvzsbcfppgsjp18gj4w-korri-plugin";
      korri-plugin-proton = "/nix/store/2qspr627wbdprz9cg81143gn2kqyrh14-korri-plugin";
      korri-plugin-retroarch = "/nix/store/kr7n1af5h2wk34grr26wcb82a2cgyasz-korri-plugin";
      korri-plugin-retrobrews = "/nix/store/zg4s6g5ad115lfzpq3g3pfil6v4ml1zn-korri-plugin";
      korri-plugin-snes9x = "/nix/store/3d4z2bhg6r1vi7rmw3b7a0zp8ymrn8bb-korri-plugin";
      korri-plugin-solarus = "/nix/store/kcqg46vamrbhxl9la0vn15qxdh9ms2m7-korri-plugin";
      korri-plugin-ssh = "/nix/store/7frsp72c18bp69vjr7avs0l4lmjankb5-korri-plugin";
      korri-plugin-starter-pack = "/nix/store/6jss7lmpqwn5a5vnngs1rjh8n6vfgr71-korri-plugin";
      korri-plugin-steam = "/nix/store/7ks62xvwfl172rl7i4z8fmn8iiyngaaq-korri-plugin";
      korri-plugin-stella = "/nix/store/xbs91n6qb36rd2rkwfxqr3sgbh7yh7s2-korri-plugin";
      korri-plugin-sunshine = "/nix/store/znzzy4dg34rfdlci52wy65zglccsx2hc-korri-plugin";
      korri-plugin-zquest-classic = "/nix/store/v45niqywg5nxnpzrpf1xwmghxs3467ih-korri-plugin";
    };
    x86_64-linux = {
      korri-plugin-beetle-ngp = "/nix/store/hns8ihzwqca6wc5rl205ck28cvnsapcn-korri-plugin";
      korri-plugin-beetle-pce-fast = "/nix/store/yf8l20q5w6kmrj52rkm6ifkr1dp464cj-korri-plugin";
      korri-plugin-beetle-wswan = "/nix/store/x027391sg9cym9w6rfh0wy8q7qxci01x-korri-plugin";
      korri-plugin-fake08 = "/nix/store/s038hksv5dvhhj25r22hy87ij5c1xx69-korri-plugin";
      korri-plugin-fbneo = "/nix/store/g1d3rhijivf4zkm1amyk0p76my8l6fjb-korri-plugin";
      korri-plugin-fex = "/nix/store/w0nzpwryn6ckkfrrwka0lbm5iksm3y15-korri-plugin";
      korri-plugin-flycast = "/nix/store/w02lk6jd6pzqbsfqvcl217zvks9iscv4-korri-plugin";
      korri-plugin-gambatte = "/nix/store/7dfmgjaym6i4283qdkblrijnml8znchn-korri-plugin";
      korri-plugin-gamescope = "/nix/store/y79gdxymlw9f0376x9j482shslc9ygxa-korri-plugin";
      korri-plugin-genesis-plus-gx = "/nix/store/064sw1xygi67ribdrndby81ga9krgqa5-korri-plugin";
      korri-plugin-gw = "/nix/store/0wwizrmlk72xxa4rmnl29dwka00w0mxp-korri-plugin";
      korri-plugin-handy = "/nix/store/s6lyq3zzcwdmj88d8s5knz51l6gcb4vk-korri-plugin";
      korri-plugin-homebrewhub = "/nix/store/fdk5i6vnxbwmq0a1jx7kpp64zi33fx08-korri-plugin";
      korri-plugin-melonds = "/nix/store/ghsz6gznr76agv6k85hjj8sd4g1lclva-korri-plugin";
      korri-plugin-mgba = "/nix/store/kbnj3vqr3rjin17rijhh2j3plcrq7skv-korri-plugin";
      korri-plugin-moonlight = "/nix/store/92wjmvjlr9pqnqks2cwv4s2klg42vz95-korri-plugin";
      korri-plugin-mupen64plus = "/nix/store/cw9l6bm480vwmdn174kx75zccfmpk1rq-korri-plugin";
      korri-plugin-nestopia = "/nix/store/9p9q03yb4nnvsaxqxhljvi5la95w07q2-korri-plugin";
      korri-plugin-pcsx-rearmed = "/nix/store/pajciplk4is9f3nxlsdwbcahs0ydzzmy-korri-plugin";
      korri-plugin-pico8 = "/nix/store/ymsxs9wj428qldpchpmrz3wixjgkfkk4-korri-plugin";
      korri-plugin-picodrive = "/nix/store/7sppcd5m9bfhvnjfl9qbg31wxdzs3sk4-korri-plugin";
      korri-plugin-prosystem = "/nix/store/166phvi03c5673q607q3g91ph71z2xgl-korri-plugin";
      korri-plugin-proton = "/nix/store/rjan002x2hvpk9n0ncwzxvzfvr25ri6v-korri-plugin";
      korri-plugin-retroarch = "/nix/store/b7lhw2hl4b7mqsj0z8cwc570d4axv2s8-korri-plugin";
      korri-plugin-retrobrews = "/nix/store/mkfap2znlvqg5ralffl3admjkv1mp38k-korri-plugin";
      korri-plugin-snes9x = "/nix/store/l6lnkqkfw87zd13y2xc0zrvlf2mz8k5n-korri-plugin";
      korri-plugin-solarus = "/nix/store/gpfrlzrq9ixm1kpxwswpymgr7vzpi8sm-korri-plugin";
      korri-plugin-ssh = "/nix/store/l81yra5g4122wm8cpv2pdpgqjw5ws5p1-korri-plugin";
      korri-plugin-starter-pack = "/nix/store/549pgpkjh91p7x6vzby44899l3g5fp55-korri-plugin";
      korri-plugin-steam = "/nix/store/3k5pfp2c1di8rkav0dr6r7j79fqfp98d-korri-plugin";
      korri-plugin-stella = "/nix/store/42r7am2gxylzvhmh9id82fkcljlc3c1c-korri-plugin";
      korri-plugin-sunshine = "/nix/store/6p5c6w5401wawck7hyhzc9bbfz6qmkdh-korri-plugin";
      korri-plugin-zquest-classic = "/nix/store/kiajz5n1mqwf3x44751lnma5x94wa5zv-korri-plugin";
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

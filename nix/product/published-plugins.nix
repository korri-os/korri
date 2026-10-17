# Exact immutable publisher outputs, not recipes evaluated with Core.
# Grounding: build-9514b1b382d3
# paths-SYSTEM.txt and revision.txt from their immutable releases;
# IDs below come from the current host seed report over each published package.
# Plugin updates change these pins and matching offline proof assets explicitly.
{ system }:
let
  published = {
    aarch64-linux = {
      korri-plugin-beetle-ngp = "/nix/store/ck12yzcq0bf5k1jkavpcyrijw6jm9jjd-korri-plugin";
      korri-plugin-beetle-pce-fast = "/nix/store/qnhk5fn3d5hdmsm5gh0mx3y1a1l5glj9-korri-plugin";
      korri-plugin-beetle-wswan = "/nix/store/q4a5p64ldk7i2wm8j5agdrk61lkdfdz1-korri-plugin";
      korri-plugin-fake08 = "/nix/store/66c4hds7g34kra7jkl53wmrm42p60v2w-korri-plugin";
      korri-plugin-fbneo = "/nix/store/060wwk3q1n6h9ixjqdkrm4vdrwz2fx34-korri-plugin";
      korri-plugin-flycast = "/nix/store/1hyibappvdbx4jmqdsrp5blyyba46wkv-korri-plugin";
      korri-plugin-gambatte = "/nix/store/wqaq8k7vgb7snzl9i8q448d3n913b5h4-korri-plugin";
      korri-plugin-genesis-plus-gx = "/nix/store/8g6n55diwvblwcs3hl35k1n03j7b34yk-korri-plugin";
      korri-plugin-gw = "/nix/store/b44l2y0gcj8h5fqwmhhfq9wszwfdqid1-korri-plugin";
      korri-plugin-handy = "/nix/store/0fx1xk6n4c2lp4k3jrhnzpyk6i9d5vnn-korri-plugin";
      korri-plugin-melonds = "/nix/store/r269nhzzyl6c315yzk11ga4w78q8nw0v-korri-plugin";
      korri-plugin-mgba = "/nix/store/zldm297c5zysmsa0krsipqas2zh3h4gk-korri-plugin";
      korri-plugin-mupen64plus = "/nix/store/18bbsh7scpmhv40nlixlsbzn78ig07qk-korri-plugin";
      korri-plugin-nestopia = "/nix/store/w9hwk6yv6s95zrqgcn9kp8lz8sks8knz-korri-plugin";
      korri-plugin-pcsx-rearmed = "/nix/store/mprp6pbcyi45w84bc6ilwyz62z0ac249-korri-plugin";
      korri-plugin-picodrive = "/nix/store/3bmsv149dbr0i5r9frim9ivilc7vsa9n-korri-plugin";
      korri-plugin-prosystem = "/nix/store/plh3rwpkiy6y7ir6kmj4mhqn464p1da4-korri-plugin";
      korri-plugin-retroarch = "/nix/store/1s65pcy9clbhh7jiav9nv692drh4y6dp-korri-plugin";
      korri-plugin-snes9x = "/nix/store/siyksarqi9j4bnh2wxhk6zxyhyp2awci-korri-plugin";
      korri-plugin-solarus = "/nix/store/2yp6hpj8bk6h78h3p46cq51af6p36kq3-korri-plugin";
      korri-plugin-ssh = "/nix/store/7frsp72c18bp69vjr7avs0l4lmjankb5-korri-plugin";
      korri-plugin-starter-pack = "/nix/store/a9jk1mfv10qs20xf2skq32imjhw9jq51-korri-plugin";
      korri-plugin-stella = "/nix/store/hmyzqn91nvaq3vjnxn7m1ycvwizj2k36-korri-plugin";
      korri-plugin-sunshine = "/nix/store/7q3fv7767vdzrbax2v2dz2chcvp2yf7r-korri-plugin";
    };
    x86_64-linux = {
      korri-plugin-beetle-ngp = "/nix/store/fpb9mpv7gbmyy5br87amgcfqplb9y601-korri-plugin";
      korri-plugin-beetle-pce-fast = "/nix/store/1xv4gql02dx3cgyivkn7v1c9a8da7gd2-korri-plugin";
      korri-plugin-beetle-wswan = "/nix/store/0n67mh4v9liypc2xa9gjsn7avpdl14nv-korri-plugin";
      korri-plugin-fake08 = "/nix/store/943x8pcmik2qbn04ckxq8c5qnykxjznn-korri-plugin";
      korri-plugin-fbneo = "/nix/store/xmar9arskgwsk7bfldi1x111s7mjlnch-korri-plugin";
      korri-plugin-flycast = "/nix/store/ysp64r0bv0pyvnnk6cj03j7d65xfl32q-korri-plugin";
      korri-plugin-gambatte = "/nix/store/989m522gm214hcjq8b6rgx4z677lvx5c-korri-plugin";
      korri-plugin-genesis-plus-gx = "/nix/store/725vhfqq0zssspafbrkn4ffy0z1g5d3s-korri-plugin";
      korri-plugin-gw = "/nix/store/hr7fccqr7vz02k91i3vjiggq60lzyr3k-korri-plugin";
      korri-plugin-handy = "/nix/store/rbxh6ylhzlyxslxy85m13yiy54d584k2-korri-plugin";
      korri-plugin-melonds = "/nix/store/1y3ywmyrq1xc6xhdy85yyvjzfbsd9x2x-korri-plugin";
      korri-plugin-mgba = "/nix/store/lvwhsc02xk0r8sqxvsnjgsyaap05ab5x-korri-plugin";
      korri-plugin-mupen64plus = "/nix/store/ab1zbr2wgxqb7dhll40b46iph20j06s8-korri-plugin";
      korri-plugin-nestopia = "/nix/store/56hdvs405d1d65nmbbaaizz1ij9czl21-korri-plugin";
      korri-plugin-pcsx-rearmed = "/nix/store/4ladfsvldvyxmma4phkzbvrrr4p8qwzw-korri-plugin";
      korri-plugin-picodrive = "/nix/store/sp14qmpyzd1q1i0sqklaysijl942df96-korri-plugin";
      korri-plugin-prosystem = "/nix/store/gz19rphfd55k6jk2q7d1whn1l41g200b-korri-plugin";
      korri-plugin-retroarch = "/nix/store/dd28fs7hb6jkjjyspc69z6s4gif8wfjg-korri-plugin";
      korri-plugin-snes9x = "/nix/store/x50lsky8acip2kv41x7kr7677gnkm01l-korri-plugin";
      korri-plugin-solarus = "/nix/store/8n2ar4s593bz07ych8ld4w6ln04mnv1h-korri-plugin";
      korri-plugin-ssh = "/nix/store/l81yra5g4122wm8cpv2pdpgqjw5ws5p1-korri-plugin";
      korri-plugin-starter-pack = "/nix/store/frrydsyhw30s9sx1121x3w5089ybay4s-korri-plugin";
      korri-plugin-stella = "/nix/store/67g1hi8pcwpiiba2z7i6mck0x50zvx5v-korri-plugin";
      korri-plugin-sunshine = "/nix/store/l6fi46y2m5cwgww7y7mpyriyj0z09d33-korri-plugin";
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

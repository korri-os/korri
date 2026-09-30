# Exact immutable publisher outputs, not recipes evaluated with Core.
# Grounding: build-e5e27ed406f2 and build-9f946f33bd18 paths-SYSTEM.txt;
# IDs below come from the current host seed report over each published package.
# Plugin updates change these pins and matching offline proof assets explicitly.
{ system }:
let
  published = {
    aarch64-linux = {
      korri-plugin-beetle-ngp = "/nix/store/2ksfrzyxz5l99cf5h4ysl62pqm8lgw0a-korri-plugin";
      korri-plugin-beetle-pce-fast = "/nix/store/35b9nw7cqrxlrb15q3dhypb1bywaicvw-korri-plugin";
      korri-plugin-beetle-wswan = "/nix/store/s7lbqxvy3140kxax9h6hcb4w4dr840h8-korri-plugin";
      korri-plugin-fbneo = "/nix/store/c16d1yjwnzk5g5h43zh821bgrnxrpvrf-korri-plugin";
      korri-plugin-flycast = "/nix/store/l2s5gaz0g9mr8539vbphgfq5rp4g6gxm-korri-plugin";
      korri-plugin-gambatte = "/nix/store/0gk9m920vs8ffby2i1qikmq8jkfv3vb6-korri-plugin";
      korri-plugin-genesis-plus-gx = "/nix/store/zr3hvwv3xn744kr0c8n0f8fasvsksmn8-korri-plugin";
      korri-plugin-gw = "/nix/store/m0hy5hh1s0mk2hmvw7wvhzb6rnkxc47g-korri-plugin";
      korri-plugin-handy = "/nix/store/nfnsv8avja9lijnfshrbpsym5pnpdl9b-korri-plugin";
      korri-plugin-melonds = "/nix/store/21k0bjrvqm7c94yyvr81va6fjr894nwq-korri-plugin";
      korri-plugin-mgba = "/nix/store/lcfdd736ia07yxby28yka0qlcqw01bmp-korri-plugin";
      korri-plugin-mupen64plus = "/nix/store/bhfwvzdzkdxi15krl3404l0vd5wphv3x-korri-plugin";
      korri-plugin-nestopia = "/nix/store/4y7c1am9l60j4jcppgxa2m1qmb3nhv7w-korri-plugin";
      korri-plugin-pcsx-rearmed = "/nix/store/p84gf5yh6hrzhlf973pz027xib63x53j-korri-plugin";
      korri-plugin-picodrive = "/nix/store/pvd8p95fsdvx892hp8xwqygc2j18nfc8-korri-plugin";
      korri-plugin-prosystem = "/nix/store/wf5hj2ikwjkjbq0lzn03q4279rdlxfjl-korri-plugin";
      korri-plugin-retroarch = "/nix/store/ikn79k1nqw7z68m6w3jizx88lqpvkxhq-korri-plugin";
      korri-plugin-snes9x = "/nix/store/ywy7fzb558s18hb863dxdd5fi8w5lsbd-korri-plugin";
      korri-plugin-ssh = "/nix/store/bqpw9scd831aqf0727gzlkp4l4al8hc1-korri-plugin";
      korri-plugin-stella = "/nix/store/rgr6hh84jd81dw7v22h4lwnwcwpv03rz-korri-plugin";
      korri-plugin-sunshine = "/nix/store/33w3ig2ilv4fhblc4gi4aa72l4y8xh06-korri-plugin";
    };
    x86_64-linux = {
      korri-plugin-beetle-ngp = "/nix/store/y3gqgc9falj0a14b0896hv0fa0p48zsr-korri-plugin";
      korri-plugin-beetle-pce-fast = "/nix/store/50dafdjniqx25cr4cpfanizw636j1qkp-korri-plugin";
      korri-plugin-beetle-wswan = "/nix/store/kavsz9nmx6a0c57f8n0s96mxvfzz5ff8-korri-plugin";
      korri-plugin-fbneo = "/nix/store/3ai78klmfvhacxz2yrzj3as832srqcmv-korri-plugin";
      korri-plugin-flycast = "/nix/store/cpk4600aqd5m3dccnnvp7b76lkhs98p3-korri-plugin";
      korri-plugin-gambatte = "/nix/store/h3j6bzf8i95g7lj141skpwqfd2ycifb3-korri-plugin";
      korri-plugin-genesis-plus-gx = "/nix/store/935z6idxxwmvrkmary9y9qq3czywvsh6-korri-plugin";
      korri-plugin-gw = "/nix/store/7rwd0zqjqb6w2mik4a3kg84q13kz2iyq-korri-plugin";
      korri-plugin-handy = "/nix/store/riqsc695b37d6f9407hrnqbjlpwd2c0q-korri-plugin";
      korri-plugin-melonds = "/nix/store/kh8fnh7w9avgacgpnf62bxai8jhcqkl5-korri-plugin";
      korri-plugin-mgba = "/nix/store/aqh981jgs92djvgnifn6a3isc7kc9r62-korri-plugin";
      korri-plugin-mupen64plus = "/nix/store/5jwvh7w7aqd62nnqy017msn3crg27117-korri-plugin";
      korri-plugin-nestopia = "/nix/store/g8pqpy628hsni0z0m7s114yl95n8mks5-korri-plugin";
      korri-plugin-pcsx-rearmed = "/nix/store/27k40lc5f4b3j0jy4q7vzwlnjs0g9rbq-korri-plugin";
      korri-plugin-picodrive = "/nix/store/2qnrddfgq5yvbzh9llp03y7436vnfjml-korri-plugin";
      korri-plugin-prosystem = "/nix/store/n4qvqb69lifd3c1i6s2sa7n7ld0scc4f-korri-plugin";
      korri-plugin-retroarch = "/nix/store/fj5nq1m7cfz3jn3162abkvm2i3l23gqa-korri-plugin";
      korri-plugin-snes9x = "/nix/store/xa0xd2sp2n9rflpak721wc17xcsla5nz-korri-plugin";
      korri-plugin-ssh = "/nix/store/aan9s1iw013h42ssm663xcid7q040pqs-korri-plugin";
      korri-plugin-stella = "/nix/store/1d2rzpnv3r9dp5slkka6cba1kggzk5ch-korri-plugin";
      korri-plugin-sunshine = "/nix/store/1m72m5hrmgq2163nnvn7py537452jyxy-korri-plugin";
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

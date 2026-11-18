# The PC package's notices (#521): one UTF-8 text with the licence of each
# part of the package that other people wrote and that its recipe builds,
# for one feature set. default.nix installs it as
# share/licenses/korri-viewer/NOTICES.txt beside the GPL text (LICENSE), and
# the corresponding source carries the default package's as
# licenses/NOTICES.txt.
#
# Its parts, in order:
# - moonlight-common-c, the C network core that default.nix builds as its own
#   library (`network`), with ENet and its Reed-Solomon code inside;
# - with nightjar-core: the GPL-3.0 crates of Nightjar and Rubylight, the
#   GPL-3.0 projects nightjar-core ports code from, and nanors, whose
#   Reed-Solomon code it ports (vendor/nanors-licence.nix);
# - the Rust standard library: Rust's own notice and licences;
# - the Rust crates of the binary for its target and features, by
#   cargo-about (about.toml, about.hbs).
#
# The carried C of picture/ and io/ is Moonlight Embedded's and Korri's,
# under the package's own licence. The libraries that the binary links from
# nixpkgs (FFmpeg, libva, SDL, libcurl, OpenSSL, libopus, ALSA, PulseAudio,
# libevdev, systemd's libudev, X11, libglvnd) are shared libraries with
# packages, sources and licences of their own.
{
  pkgs,
  # The repository files that build the viewer (default.nix, sourceFiles).
  src,
  # The vendored crates of Cargo.lock.
  cargoDeps,
  # Moonlight Embedded's source, whose moonlight-common-c default.nix builds.
  upstream,
  features,
  version,
}:
let
  inherit (pkgs) lib;
  target = pkgs.stdenv.hostPlatform.rust.rustcTarget;
  nightjar = lib.elem "nightjar" features;
  moonlight = "${upstream}/third_party/moonlight-common-c";
  nanorsLicence = import ./vendor/nanors-licence.nix { inherit pkgs; };
  # The Rust that builds the binary, whose standard library the binary holds.
  rustc = pkgs.rustPlatform.rust.rustc;
in
pkgs.stdenv.mkDerivation {
  pname = "korri-viewer-notices${
    lib.optionalString (features != [ ]) "-${lib.concatStringsSep "-" features}"
  }";
  inherit version src cargoDeps;
  sourceRoot = "source/clients/viewer";
  nativeBuildInputs = [
    pkgs.cargo
    pkgs.rustPlatform.cargoSetupHook
    pkgs.cargo-about
  ];
  dontConfigure = true;
  buildPhase = ''
    runHook preBuild
    export HOME=$TMPDIR
    cargo about generate --frozen --fail --target ${target} \
      ${lib.optionalString (features != [ ]) "--features ${lib.concatStringsSep "," features}"} \
      -c ${./about.toml} -o crates.txt ${./about.hbs}

    # A licence file, which must be there and not be empty.
    licence() {
      if [ ! -s "$1" ]; then
        echo "notices: no licence text at $1" >&2
        exit 1
      fi
      printf '\n'
      cat "$1"
    }
    # A part: its title, what it is and where it is, then its licence files.
    part() {
      local title="$1" about="$2"
      shift 2
      printf '\n================================================================================\n%s\n\n%s\n' "$title" "$about" |
        fold -s -w 80 | sed 's/[[:space:]]*$//'
      for file in "$@"; do licence "$file"; done
    }

    {
      printf 'korri-viewer %s for %s: the parts that other people wrote\n\n' ${version} ${target}
      printf '%s\n' ${
        if nightjar then
          "'The binary is under the GNU General Public License version 3 only (GPL-3.0-only), as it links nightjar-core.'"
        else
          "'The binary is under the GNU General Public License version 3 or later (GPL-3.0-or-later).'"
      } "The text of that licence is LICENSE, beside this file. The carried C of picture/ and io/ is Moonlight Embedded's and Korri's, under it too. The parts below are the ones that other people wrote and that the package's recipe builds in." |
        fold -s -w 80 | sed 's/[[:space:]]*$//'

      # Its licence must still be the GPL version 3, whose text is LICENSE.
      grep -q 'Version 3, 29 June 2007' ${moonlight}/LICENSE.txt
      part 'moonlight-common-c' \
        "The C network core, from Moonlight Embedded, built unmodified as the package's own library. GNU General Public License version 3; the text is LICENSE, beside this file."
      part 'ENet' \
        'The network library inside moonlight-common-c.' \
        ${moonlight}/enet/LICENSE
      # The licence is the head comment of the file.
      sed -n '1,/\*\//p' ${moonlight}/reedsolomon/rs.c > reed-solomon.txt
      part 'Reed-Solomon forward error correction' \
        "The FEC code inside moonlight-common-c (reedsolomon/rs.c)." \
        reed-solomon.txt
      ${lib.optionalString nightjar ''
        # Both projects' own licence files must still be the GPL version 3.
        for file in vendor/nightjar/LICENSE.txt vendor/rubylight-native-api/LICENSE; do
          grep -q 'GNU GENERAL PUBLIC LICENSE' "$file" && grep -q 'Version 3, 29 June 2007' "$file" ||
            { echo "notices: $file is not the GPL version 3" >&2; exit 1; }
        done
        part 'nightjar-core and the Rubylight crates' \
          "The stream's default network core: nightjar-core and the crates rubylight-protocol and rubylight-traverse, from Nightjar (https://github.com/RamazanKara/nightjar), and rubylight-native-api, from Rubylight (https://github.com/RamazanKara/Rubylight), compiled into the binary from Korri's copies in clients/viewer/vendor, whose README names their commits and Korri's changes. GNU General Public License version 3 only; the text is LICENSE, beside this file. nightjar-core ports code from these GPL-3.0 projects, as its source files name them: moonlight-common-c and moonlight-android by Cameron Gutman and contributors, ClassicOldSong's moonlight-common-c (Artemis), Moonlight V+ and Moonlight V+ PC by qiin2333, and Nonary's moonlight-common-c. Its Reed-Solomon code is ported from nanors, under the MIT License (the next part). Nightjar's changed copy of the crate rusty_enet is among the Rust crates below."
        part 'nanors' \
          "Reed-Solomon erasure coding by Joseph Calderon (https://github.com/sleepybishop/nanors), which nightjar-core ports to Rust for the stream's video and audio FEC, as moonlight-common-c vendors it (clients/viewer/vendor/nightjar/protocol/client/src/fec.rs). nanors' licence, at its commit 0b09d1ee, follows." \
          ${nanorsLicence}
      ''}
      # Rust's source ships its notice and its two licences; nixpkgs' build of
      # Rust does not make the per-crate COPYRIGHT-library.html of Rust's own
      # releases.
      rust=$(mktemp -d)
      tar -xzf ${rustc.src} -C "$rust" --wildcards '*/COPYRIGHT' '*/LICENSE-APACHE' '*/LICENSE-MIT' --exclude='*/*/*'
      part 'The Rust standard library ${rustc.version}' \
        "Rust's core, alloc and std, compiled into the binary by the Rust that built it. Rust's own notice and its licences, from its source, follow; Rust's releases list the crates that its standard library is built from in COPYRIGHT-library.html." \
        "$rust"/*/COPYRIGHT "$rust"/*/LICENSE-APACHE "$rust"/*/LICENSE-MIT

      printf '\n\nRUST CRATES IN THE BINARY (%s%s)\n' ${target} ${
        lib.optionalString (features != [ ]) "', features ${lib.concatStringsSep "," features}'"
      }
      cat crates.txt
    } > NOTICES.txt

    # The vendored trees' licence mentions all have their part here.
    ${lib.optionalString nightjar "bash vendor/check-licence-mentions.sh vendor NOTICES.txt"}
    runHook postBuild
  '';
  installPhase = ''
    runHook preInstall
    install -D -m 0644 NOTICES.txt $out/NOTICES.txt
    runHook postInstall
  '';
}

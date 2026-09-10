{ pkgs }:
pkgs.runCommand "korri-image-dist-check"
  {
    nativeBuildInputs = [
      pkgs.python3
      pkgs.zstd
    ];
  }
  ''
    cp ${./image-dist.py} image-dist.py
    cp ${./image-dist.test.py} image-dist.test.py
    python3 image-dist.test.py
    touch "$out"
  ''

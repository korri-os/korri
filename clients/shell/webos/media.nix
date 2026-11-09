# The approved main executable links the pinned viewer core and retains media-test.
{ pkgs }:
(import ../../webos-viewer { inherit pkgs; }).cross

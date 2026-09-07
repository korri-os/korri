# Legacy's asset-root/URL/profile environment is preserved. Bundle selection
# uses Nix's named-profile producer rather than a new deployment manifest.
{
  assetRoot = "/nix/var/nix/profiles/korri-portal";
  host = "127.0.0.1";
  port = 8099;
  chromiumProfileSuffix = "/.local/state/korri/chromium/profile";
}

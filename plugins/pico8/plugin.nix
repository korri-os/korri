# A source-only acquisition provider. korrid makes every request for it and
# refuses any host not named here, on every redirect hop and for the cart
# download itself. Search, post pages and cart files all live on this host.
{ pkgs }:
{
  hosts = [ "www.lexaloffle.com" ];
}

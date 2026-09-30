# Device-gate approval fixtures

These test fixtures preserve the existing device gate's approval reference.
They contain the nine `EXPECTED_SUNSHINE_*` lines and sixteen ordered `patch=`
records from `services/inputd/deploy/device-check.sh` at Core commit
`f6597dd6fd8cd1452f5f75803672b33a8cfd6225`. These values also matched the
producer approval record used by the package check before the ownership cut.
No values, keys, or record format were added.

The fixtures are test inputs, not runtime approval or publisher metadata.
The package check still compares the installed gate with its complete source.
It also compares the approval lines and ordered patch records with these fixed
fixtures. A publisher's new native provenance cannot update device policy.
Changing runtime approval remains a separate reviewed operation.

The current gate accepts the x86 CUDA wrapper layout. It refuses the selected
ARM binary symlink layout. The resolver tests keep both results explicit.
This ownership change does not add ARM deployment support to that gate.

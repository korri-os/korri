# Retain the native candidate for manual testing

The user objected that the automated component trial restored the original stack before manual testing. The earlier trial result remains valid, but it was not manual acceptance. On request to correct this, restage the same prebuilt core candidate and leave it running.

The new explicit `manual` mode starts the candidate through the existing guarded transaction, requires exact readiness and a second actual verification, and then disarms automatic rollback only on success. Failed setup or failed verification still attempts rollback. No replay is injected. The original snapshots and explicit restore command remain available. A user session must end before the existing idle-only restore can run.

This retains temporary selection under `/run`. It is not a permanent installation. Signed Sunshine remains stopped and masked during the native physical-input test; remote streaming is unavailable. Plugin receipts and signatures remain unchanged. Do not run plugin restore/install commands while the temporary candidate owns the input stack.

Tests execute the actual shell finish handler in subprocesses. Manual success calls only verification, never restore. Manual verification failure reports failure after restore/verify. Automatic trials and failed setup retain their original rollback behavior.

Status: candidate is running for the user's manual test. Parent process `proc_09a9` completed startup and a separate actual `manual-check`, both successfully. The stage is `/run/korri-input-trial.URiPGSUr`. It verified candidate executables, inputd readiness, kiosk state, runtime masks and idle RPC. No replay ran and no rollback timer is armed. The workstation record is `/tmp/korri-native-manual/`; its `restore-command` contains the exact explicit recovery command. Leave the candidate running until the user directs otherwise.

An earlier staging-only attempt included a workstation-generated Python bytecode directory and failed the public-mode guard before service changes. The fresh payload excludes bytecode. Thirty-two host tests passed, including three actual shell-finish subprocess cases. Manual button and gameplay results still require the user's report.

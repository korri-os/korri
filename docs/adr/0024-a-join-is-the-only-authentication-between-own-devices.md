# A join is the only authentication between a person's own devices

**Owner decision, 2026-10-22.**

Once a device has joined a person's devices, the person never authenticates
anything else between those devices. Save sync, peer access, endpoint
discovery and Moonlight streaming trust all follow from the owner statements
and happen without a prompt, a PIN or a code. Any feature that would ask the
person to pair, approve or type something again between two of their own
devices is a defect against this decision.

The first concrete consequence: a Korri device provisions its Moonlight client
certificate on the host through korrid by itself. korrid already grants that
certificate to owner devices (`app.moonlight.certificate.provision`); on
2026-10-22 no production client calls it.

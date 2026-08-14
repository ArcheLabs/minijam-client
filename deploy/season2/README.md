# Season 2 deployment profiles

Season 2 is an Experience Network. The compact profile runs one node, one
worker, the Experience API, compiler, and allocation relay on one host while
keeping the node, API, and compiler on separate Docker network surfaces. The
split profile uses the same runtime and separates the node/worker host from
the API/compiler host over a private `node-net` connection.

Both profiles use `--rpc-methods=safe`. The compiler has no validator,
allocation-relayer, or sudo key mounts. The relayer key belongs only to the
API/relay process. Configure production images and secrets through environment
variables; the example values are for local development only.

The runtime does not expose a MiniJAM-to-Hub release operation. Allocation
redemption remains entirely a Hub concern.

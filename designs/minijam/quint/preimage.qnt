// Preimage registry abstraction.
module preimage {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  pure def preimageHash(local: int): Hash = hashOf(1000 + local)

  pure def applyPreimageChange(s: MiniJamState, change: StateChange): MiniJamState =
    if (change.key.namespace != Preimage) s
    else match change.op {
      | Remove => { ...s, preimages: mapRemove(s.preimages, preimageHash(change.key.local)) }
      | _ => match change.value {
        | Some(v) => { ...s, preimages: s.preimages.put(preimageHash(change.key.local), v) }
        | None => s
      }
    }
}

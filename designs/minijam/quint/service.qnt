// Service storage helpers.
module service {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  pure def serviceStorageKey(serviceId: ServiceId, change: StateChange): (ServiceId, int) =
    (serviceId, change.key.local)

  pure def applyServiceChange(s: MiniJamState, serviceId: ServiceId, change: StateChange): MiniJamState =
    if (change.key.namespace == ServiceStorage or change.key.namespace == ServiceInfo or change.key.namespace == ServiceLookup) {
      val key = serviceStorageKey(serviceId, change)
      val lookupSet =
        if (change.key.namespace != ServiceLookup) s.serviceLookups
        else if (change.op == Remove) s.serviceLookups.exclude(Set(key))
        else s.serviceLookups.union(Set(key))
      val withLookup = { ...s, serviceLookups: lookupSet }
      match change.op {
        | Remove => { ...withLookup, serviceStorage: mapRemove(withLookup.serviceStorage, key) }
        | _ => match change.value {
          | Some(v) => { ...withLookup, serviceStorage: withLookup.serviceStorage.put(key, v) }
          | None => withLookup
        }
      }
    } else s
}

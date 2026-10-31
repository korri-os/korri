// THROWAWAY: Map deliberately proves state is not JSON-round-tripped.
globalThis.surface = {
  init: (_model) => new Map([["position", 0]]),
  update(state, message, _model) {
    const delta = message.action === "right" ? 1 : message.action === "left" ? -1 : 0;
    return new Map([["position", state.get("position") + delta]]);
  },
  view: (state, model) => ({ position: state.get("position"), coverImage: { $image: model.catalog.games[0].coverArtUrl } }),
};

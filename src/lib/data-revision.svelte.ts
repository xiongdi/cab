/** Bump counters when catalog data changes so dependent pages can reload. */
class DataRevision {
  models = $state(0);
  providers = $state(0);

  touchModels() {
    this.models++;
  }

  touchProviders() {
    this.providers++;
  }
}

export const dataRevision = new DataRevision();

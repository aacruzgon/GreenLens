// The one seam between the api layer and the network (docs/frontend/README.md §9).
// openapi-fetch calls fetch with a Request object; expo/fetch and the mock both accept one.
export interface Transport {
  fetch(input: string | Request, init?: RequestInit): Promise<Response>;
}

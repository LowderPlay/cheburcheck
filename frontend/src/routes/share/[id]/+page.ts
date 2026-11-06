import type { PageLoad } from "./$types";

export const load: PageLoad = ({ params }) => ({ id: params.id });

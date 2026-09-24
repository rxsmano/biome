/* should not generate diagnostics */
import { deprecatedLast, deprecatedFirst, documentedModern, modern } from "./overloads";
import { renamed, deprecatedLast as reexported } from "./overloadReexports";
import { mixed } from "./implementedOverloads";
import mixedDefault from "./overloads";

export const value = deprecatedLast("https://example.test");

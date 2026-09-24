export declare function deprecatedLast(url: string): string;
/** @deprecated Use the modern options instead. */
export declare function deprecatedLast(url: string, options: { legacy: true }): string;

/** @deprecated Use the modern options instead. */
export declare function deprecatedFirst(url: string, options: { legacy: true }): string;
export declare function deprecatedFirst(url: string): string;

/** Uses modern options. */
export declare function documentedModern(url: string): string;
/** @deprecated Use the modern options instead. */
export declare function documentedModern(url: string, options: { legacy: true }): string;

/** @deprecated Use another function. */
export declare function allDeprecated(url: string): string;
/** @deprecated Use another function. */
export declare function allDeprecated(url: string, options: { legacy: true }): string;

/** @deprecated */
export declare function noMessage(url: string): string;
/** @deprecated */
export declare function noMessage(url: string, options: { legacy: true }): string;

export declare function modern(url: string): string;
export declare function modern(url: string, options: { modern: true }): string;

export default function mixedDefault(value: string): string;
/** @deprecated Pass a string. */
export default function mixedDefault(value: number): string;

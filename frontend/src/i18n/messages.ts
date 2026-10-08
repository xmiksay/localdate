/** The shape of a locale: same keys as `T`, any string values. */
export type Messages<T> = { [K in keyof T]: T[K] extends string ? string : Messages<T[K]> }

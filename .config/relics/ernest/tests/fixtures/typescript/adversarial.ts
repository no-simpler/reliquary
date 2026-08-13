/// <reference types="node" />
// SPDX-License-Identifier: MIT

import { readFile } from "node:fs/promises";

/** A tenant, as the API returns it. */
export interface Tenant {
    /** The identifier. */
    id: number;
    name: string;
}

// A template literal type, whose backticks hold no comment.
export type Slug = `tenant-${string}`;

/**
 * Resolves the tenant for a request.
 *
 * @param id The tenant identifier.
 * @returns The tenant, or null when there is none.
 * @deprecated Use `resolve` instead.
 */
export async function tenant(id: number): Promise<Tenant | null> {
    const url = "http://not-a-comment/#frag";
    const template = `${id} // not a comment`;
    const pattern = /\/\*[^*]*\*\//g;
    const quotient = id / 2 / 3;

    // The angle-bracket type assertion: the construct that makes this a
    // separate grammar from TSX, where it would read as an element.
    const raw = <string>(<unknown>await readFile(url, "utf8"));

    // @ts-expect-error the shape is checked at runtime
    const parsed: Tenant = JSON.parse(raw);

    const identity = <T>(value: T): T => value;
    return identity(parsed) ?? [pattern, template, quotient] && null;
}

export enum Status {
    Active = "active", // a trailing comment on an enum member
    Retired = "retired",
}

declare module "node:fs/promises" {
    // An augmentation, commented.
    interface Stub {
        marker: string;
    }
}

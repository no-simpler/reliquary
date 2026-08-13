#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// @ts-check
/* eslint-disable no-console */

/**
 * Resolves the tenant for a request.
 *
 * Every slash below that is not a comment, and every comment shape the
 * language offers.
 *
 * @param {number} id The tenant identifier.
 * @returns {string} The resolved name.
 */
export function tenant(id) {
    const url = "http://not-a-comment/#frag";
    const block = "/* not a comment */";
    const escaped = "escaped quote \" then // not a comment";
    const template = `${id} // not a comment`;
    const nested = `outer ${`inner /* nope */`} done`;

    // The division-versus-regex ambiguity, which is the lexing hazard here.
    const pattern = /https?:\/\/example\.com\/[*]/;
    const charClass = /[/*]+/g;
    const quotient = id / 2 / 3;

    return [url, block, escaped, template, nested, pattern, charClass, quotient].join("");
}

class Counter {
    // A private field is not a comment, whatever the sigil suggests.
    #count = 0;

    increment() {
        this.#count += 1; // a real comment, after a hash that is not one
        return this.#count;
    }
}

// eslint-disable-next-line no-unused-vars
const counter = new Counter();

/* istanbul ignore next */
function unreachable() {
    return null;
}

//// Four slashes are still a comment.
// =========================
export default { tenant, unreachable };

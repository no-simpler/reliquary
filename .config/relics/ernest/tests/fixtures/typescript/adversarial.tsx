// The same language as adversarial.ts, and it reports as such — only the
// grammar differs, because JSX and the angle-bracket type assertion cannot
// share one.

interface Props {
    /** Where the link points. */
    href: string;
    items: readonly string[];
}

// The generic arrow needs its trailing comma here: without it the parameter
// list reads as an element, which is exactly why this file is not measured by
// the TypeScript grammar.
const identity = <T,>(value: T): T => value;

/**
 * A panel.
 *
 * @param props The link target and the list to render.
 */
export function Panel({ href, items }: Props): JSX.Element {
    // A comment outside the markup.
    const count = items.length / 2;

    return (
        <section className="panel // not a comment" data-count={count}>
            {/* A comment inside a JSX expression is an ordinary comment. */}
            <h1>Reticulating splines</h1>
            <p>
                This copy is interface text rather than prose about the code, so
                it bills as code — the same side of the line as a string literal.
            </p>
            <ul>
                {items.map((item) => (
                    <li key={identity(item)}>{item}</li>
                ))}
            </ul>
            <a href={href} title="https://example.com/#frag">
                Read more
            </a>
        </section>
    );
}

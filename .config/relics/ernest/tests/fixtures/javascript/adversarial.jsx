/**
 * JSX rides the same grammar as plain JavaScript — no second profile.
 *
 * @param {{ href: string }} props The link target.
 */
export function Banner({ href }) {
    // A comment outside the markup.
    return (
        <section className="banner // not a comment">
            {/* A comment inside a JSX expression is an ordinary comment. */}
            <h1>Reticulating splines</h1>
            <p>
                This copy is the product rather than prose about the code, so it
                bills as code — the same side of the line as any string literal.
            </p>
            <a href={href} title="https://example.com/#frag">
                Read more
            </a>
        </section>
    );
}

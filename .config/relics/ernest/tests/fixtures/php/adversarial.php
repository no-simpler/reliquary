#!/usr/bin/env php
<?php

// A line comment.
# A hash comment.

// =========================

/**
 * Resolves the tenant for a request.
 *
 * @param int $id
 * @return Tenant
 */
function tenant(int $id): Tenant
{
    $url = "http://not-a-comment";
    $sql = <<<SQL
      -- not a comment either
      SELECT # nor this
    SQL;
    $raw = <<<'NOW'
      # still not a comment
    NOW;

    return new Tenant($id, $url, $sql, $raw); // trailing note
}

// phpcs:disable Generic.Files.LineLength

#[Attribute]
#[Route("/tenants")]
class Marker
{
}
?>
<p>Inline HTML is the template's payload, not prose.</p>
<?php echo tenant(1);

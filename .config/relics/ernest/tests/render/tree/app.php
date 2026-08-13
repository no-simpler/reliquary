<?php

// Reticulates the splines, which is what the widget is for.
function reticulate(array $splines): array
{
    return array_map(static fn ($spline) => $spline * 2, $splines);
}

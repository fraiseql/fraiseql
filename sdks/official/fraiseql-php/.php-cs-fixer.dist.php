<?php

declare(strict_types=1);

/**
 * Code style for the PHP SDK (#1238).
 *
 * This file has to exist for `php-cs-fixer check` to check anything. Without a config
 * the command does not fail — it writes a default one, prints "re-run the command to
 * put it in action", and exits 0, so `php-sdk.yml`'s "Check code style" step reported
 * success without reading a single file.
 *
 * @PSR12 is the ruleset the SDK already follows in spirit: it declares PSR-4 autoloading
 * (#1184) and its compliant files are PSR-12 shaped already. Risky fixers are left out —
 * they need `--allow-risky`, which would mean changing the workflow's command as well as
 * its configuration, and a style gate should not be the thing that rewrites semantics.
 */

$finder = (new PhpCsFixer\Finder())
    ->in(__DIR__ . '/src')
    ->in(__DIR__ . '/tests');

return (new PhpCsFixer\Config())
    ->setRules(['@PSR12' => true])
    ->setFinder($finder);

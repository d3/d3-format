#Requires -Version 5.1

<#
.SYNOPSIS
    Throws unless every tracked path covered by the .gitattributes eol rule is LF
    in the index and in the working tree.

.DESCRIPTION
    Section 1 pins the port's checkout form with `rust/** text eol=lf` and
    `canaries/** text eol=lf`, and every byte-comparison gate in section 3 rests on
    it. That rule governs checkout, and Phase 6 found what it does not govern: nine
    tracked files under those two roots had CRLF in the working tree, and git called
    the tree clean the whole time.

    Which half of git stays quiet depends on how the CRLF arrived, and both halves
    were measured on git 2.55.0 rather than assumed. A file git itself checked out as
    CRLF -- what happens to anything checked out under core.autocrlf=true before the
    rule covered it, which is every file committed before .gitattributes was added --
    matches the stat data cached in the index, so `git status` never reads its
    contents and reports nothing whatsoever. Rewrite that same file with an editor
    and the cached size stops matching, so status does print ` M`, but `git diff`
    stays empty and the content hash is unchanged, because the `text` attribute
    normalizes the bytes back to LF on the way to the index. The first case is the
    one the nine files were in.

    Neither case reaches an existing gate. Section 3.7's drift clause is scoped to
    rust/fixtures, rust/tests/generated and rust/src/locales/generated.rs, and none
    of the nine was under those paths; the collectors' clean-tree refusal reads
    `git status`, which the first case is invisible to.

    It is not cosmetic. `cargo package` reads the working tree rather than the
    index, and three of the nine -- README.md, CHANGELOG.md and COMPATIBILITY.md --
    are packaged, so the crate sha256 depended on which files an editor had last
    rewritten and would not have matched a package built from a fresh checkout of
    the same commit. Section 6.3 archives that hash as release evidence and
    .github/workflows/publish-rust.yml uploads it, so a hash that reproduces from no
    checkout at all undermines the whole chain.

    `git ls-files --eol` reports the condition in one command: for every tracked
    path it prints the index ending, the working-tree ending and the attributes in
    effect, and an ending that disagrees with the declared attribute is exactly the
    failure. This script is what turns section 1's "worth running before a release"
    into a gate.

    Three things are asserted for every path under a declared root:

    - The declared attribute is still eol=lf. A file that stopped being covered --
      by a negation, or by the rule itself being edited -- would otherwise pass this
      gate by having nothing to disagree with.
    - The working-tree ending is lf. This is the hole the gate exists for.
    - The index ending is lf. git status does report this one, so it is the cheaper
      half, but reporting both is what makes the failure diagnosable: an index
      finding and a working-tree finding have different causes and different fixes.

    An ending of `none` passes both endings tests. git spells an ending it cannot
    classify that way, which is what an empty file and a single line with no
    terminator both report; neither can carry a CRLF, so neither is a finding.
    rust/fixtures/d3-format-3.1.2/regressions.jsonl is legitimately empty and
    reports `i/none w/none` today.

    .github/workflows/*.yml is deliberately out of scope, and is `w/crlf` on a
    Windows checkout today. Those files carry no eol attribute, so this gate does
    not see them, and none was added: the hazard above cannot reach them. Nothing
    byte-compares them, `cargo package` walks the crate directory and never reaches
    .github/, and Actions reads the committed blob -- `i/lf` for all four -- rather
    than any working tree. Section 1 also scopes the rule away from the JavaScript
    product on the grounds that renormalizing the reference implementation is an
    unrelated change, and the workflows are shared with it: publish.yml is the
    existing npm publication that section 1 lists as unchanged. Declaring an
    attribute for them would change checkout behavior outside the port to buy a
    guarantee nothing needs.

    Only tracked paths are checked, because only committed content has a checked-out
    form that can drift from it. An untracked file has no index ending to disagree
    with and becomes subject to the rule when it is added.

    Authored for Windows PowerShell 5.1 as well as PowerShell 7, like ci/check.ps1,
    and it contains no Bash syntax.

.PARAMETER RepositoryRoot
    The repository to check. Defaults to the one holding this script.

.EXAMPLE
    ./ci/assert-line-endings.ps1
#>

[CmdletBinding()]
param(
    [string] $RepositoryRoot
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'

# The endings a declared-LF path may report. `none` is not a laxity: git uses it for
# a file with no line terminator at all, which cannot be a CRLF either way. Every
# other spelling git can print -- crlf, mixed, and the -text it uses for a file it
# read as binary -- is a finding.
$AcceptableEndings = @('lf', 'none')

function Get-DeclaredEolRoots {
    <#
        The roots to check, read out of .gitattributes rather than written down
        here. Same reason run-canaries.ps1 reads the pinned channel out of
        rust-toolchain.toml instead of carrying its own copy: a gate holding a second
        copy of the rule it enforces stops enforcing the rule the moment the two
        disagree, and does it silently.

        Only the root .gitattributes is read, which is the only one section 1
        describes. A per-directory file would narrow the rule without this noticing,
        but it would also have to be added deliberately, and the per-path attribute
        assertion below is what would catch it: a narrowed path keeps being listed
        and stops reporting eol=lf.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)] [string] $Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "no .gitattributes at ${Path}, so the eol rule this gate enforces does not exist"
    }

    $roots = @()
    foreach ($raw in @(Get-Content -LiteralPath $Path)) {
        $line = "$raw".Trim()
        if ($line.Length -eq 0 -or $line.StartsWith('#')) { continue }

        $fields = @($line -split '\s+')
        if ($fields.Count -lt 2) { continue }
        if (@($fields | Select-Object -Skip 1) -notcontains 'eol=lf') { continue }

        # `rust/** text eol=lf` names every path under rust/, and git ls-files wants
        # the directory. Any other pattern is handed to git as written, where it is an
        # ordinary pathspec.
        $roots += ($fields[0] -replace '/\*\*$', '' -replace '^/', '')
    }

    , @($roots | Sort-Object -Unique)
}

if ([string]::IsNullOrEmpty($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
}

$attributesPath = Join-Path $RepositoryRoot '.gitattributes'
$roots = Get-DeclaredEolRoots -Path $attributesPath
if ($roots.Count -eq 0) {
    throw "$attributesPath declares no eol=lf pattern; section 1 requires one for rust/ and one for canaries/"
}

Write-Host ''
Write-Host "==> git ls-files --eol -- $($roots -join ' ')  (line-ending gate)"

$listed = & git '-C' $RepositoryRoot 'ls-files' '--eol' '--' @roots
if ($LASTEXITCODE -ne 0) {
    throw "git -C $RepositoryRoot ls-files --eol -- $($roots -join ' ') failed with exit code $LASTEXITCODE"
}

$checked = 0
$findings = @()
foreach ($raw in @($listed)) {
    $line = "$raw"
    if ([string]::IsNullOrWhiteSpace($line)) { continue }

    # `i/<index>  w/<worktree>  attr/<attributes><padding>` and then a tab and the
    # path. The tab is the only unambiguous separator: the attribute field is
    # space-padded to a fixed width, and both it and a path may contain spaces.
    if ($line -match '^i/(?<index>\S+)\s+w/(?<worktree>\S+)\s+attr/(?<attr>.*?)\s*\t(?<path>.*)$') {
        $path = $Matches['path']
        $index = $Matches['index']
        $worktree = $Matches['worktree']
        $attributes = $Matches['attr']
    }
    else {
        throw "git ls-files --eol printed a line this gate cannot parse: $line"
    }

    $checked++

    $reasons = @()
    if (@($attributes -split '\s+') -notcontains 'eol=lf') {
        $reasons += 'the declared attribute is no longer eol=lf'
    }
    if ($AcceptableEndings -notcontains $worktree) {
        $reasons += "the working tree copy is $worktree"
    }
    if ($AcceptableEndings -notcontains $index) {
        $reasons += "the index copy is $index"
    }

    if ($reasons.Count -ne 0) {
        $findings += [pscustomobject] @{
            Path = $path
            Detail = "i/$index w/$worktree attr/$attributes"
            Reasons = ($reasons -join '; ')
        }
    }
}

if ($checked -eq 0) {
    throw (@(
        "git ls-files --eol reported no tracked path under $($roots -join ', ').",
        'Either the declared patterns no longer name anything in this repository, or this',
        'is not the repository the gate was meant to check. Both are failures: a gate over',
        'nothing passes over anything.'
    ) -join [Environment]::NewLine)
}

if ($findings.Count -ne 0) {
    # Built by appending rather than as one nested array literal: PowerShell does not
    # flatten an array written inside an array literal, and -join would render the
    # inner one as "System.Object[]".
    $message = @("the line-ending gate found $($findings.Count) of $checked tracked path(s) that do not match $($attributesPath):")
    foreach ($finding in ($findings | Sort-Object -Property Path)) {
        $message += "  $($finding.Path)"
        $message += "      $($finding.Detail) -- $($finding.Reasons)"
    }
    $message += @(
        '',
        'The rule in .gitattributes governs checkout and nothing after it, and git status',
        'is not a second opinion. A path git itself checked out as CRLF still matches the',
        'stat data cached in the index, so status reports nothing at all; a path an editor',
        'rewrote does show as modified, but git diff stays empty, because the text',
        'attribute normalizes the bytes back to LF on the way to the index. The drift',
        'clause in section 3.7 covers neither: it is scoped to the generated paths.',
        '',
        'cargo package reads the working tree rather than the index, so a packaged path in',
        'this state moves the crate sha256 that section 6.3 archives as release evidence,',
        'and the archived hash then reproduces from no checkout at all.',
        '',
        'Restore a working-tree ending by deleting the path and checking it out again, so',
        'the checkout attribute applies to it afresh:',
        '',
        '    Remove-Item <path>; git checkout -- <path>',
        '',
        'An index ending is the other half and git status does report it; fix that one',
        'with git add --renormalize <path> and review the result in its own commit.',
        '',
        'If a listed path is genuinely binary or genuinely needs CRLF, declare that in',
        '.gitattributes deliberately and this gate will follow the declaration.'
    )

    throw ($message -join [Environment]::NewLine)
}

Write-Host "    $checked tracked path(s) under $($roots -join ', ') are LF in the index and in the working tree"

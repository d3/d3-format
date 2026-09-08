#Requires -Version 5.1

<#
.SYNOPSIS
    Packages the crate, reviews what went into the .crate, and archives the listing
    as a committed artifact.

.DESCRIPTION
    Section 9's release checklist requires that `cargo package` contents are
    reviewed. A listing nobody reads is not a review, so the review is encoded here
    as a rule table instead: every packaged path must match exactly one rule that
    says what it is and why a consumer needs it, and a path that matches no rule
    fails the collector. Adding a file to the crate directory therefore cannot
    quietly add it to the distribution -- it fails here until someone classifies it,
    which is the point at which a human decides whether it belongs.

    Four further things are asserted rather than assumed:

    - The working tree is the checkout section 1 declares. ci/assert-line-endings.ps1
      runs before anything is packaged, because `cargo package` reads the working
      tree rather than the index: a tracked file carrying CRLF moves the sha256 this
      archive records, from a tree git status calls clean.
    - The `exclude` list in Cargo.toml actually held. Section 7 justifies each entry
      at length, and an exclusion that silently stopped matching would put the
      multi-megabyte fixture corpora, the JavaScript tooling and the differential
      binary into the published artifact.
    - The tarball and `cargo package --list` agree. They are two independent
      renderings of the same decision and neither is checked against the other by
      cargo.
    - The archive is under the crates.io size limit, with the headroom recorded, so
      the corpora growing back into the package is visible before a publish fails.

    Like the other collectors, every field is read out of the run rather than
    transcribed, and the script is authored for Windows PowerShell 5.1 as well as
    PowerShell 7.

.PARAMETER SkipPackage
    Reuse the .crate already in target/package/ instead of building a new one.
    Intended for a caller that has just packaged, such as canaries/run-canaries.ps1;
    the collector still refuses a tarball older than the crate sources.

.PARAMETER AllowDirty
    Collects from a working tree that does not match HEAD, and passes --allow-dirty
    to `cargo package` so that cargo permits it too.

    Without it the collector refuses a dirty tree. The archive names the commit it
    measured, and naming a commit is only worth something if a reader can check that
    commit out and get the same result -- which is exactly what a collection taken
    over uncommitted work cannot offer, because the tree it measured exists nowhere
    in history. That failure mode is not hypothetical here: the package sha256 is a
    function of the commit, since cargo writes it into .cargo_vcs_info.json, so a
    dirty listing records a hash nobody can ever reproduce.

    The switch exists for local experimentation and marks the artifact:
    provenance.allow_dirty is true and provenance.git_worktree_dirty records what the
    tree actually was. An archive with either set is not release evidence.

.PARAMETER OutputPath
    Where to write the archive. Defaults to ci/package/package-listing.json.

.EXAMPLE
    ./ci/package/collect-listing.ps1 -AllowDirty
#>

[CmdletBinding()]
param(
    [switch] $SkipPackage,

    [switch] $AllowDirty,

    [string] $OutputPath
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'

# crates.io rejects an upload above this. The limit is per-crate and has been 10 MiB
# for the life of the registry; section 7 cites it as the reason fixtures/ is
# excluded, so the number belongs in the artifact rather than only in prose.
$CratesIoMaxBytes = 10485760

# Every packaged path must match exactly one of these. Order matters only in that
# the collector reports a path matched by more than one as an error: two rules
# claiming the same file means the table has stopped describing the package.
$PackageRules = @(
    [pscustomobject] @{
        Pattern = '^Cargo\.toml$'
        Category = 'manifest'
        Reason = 'The manifest cargo normalized for publication. Not the authored one; that is Cargo.toml.orig.'
    },
    [pscustomobject] @{
        Pattern = '^Cargo\.toml\.orig$'
        Category = 'manifest'
        Reason = 'The authored manifest, kept by cargo so the normalization is auditable. Cargo writes it; it cannot be excluded.'
    },
    [pscustomobject] @{
        Pattern = '^Cargo\.lock$'
        Category = 'manifest'
        Reason = 'The resolved graph. A consumer ignores it, but it is what makes `cargo test` inside the unpacked crate reproduce the validated build, which is section 9s independent-build check.'
    },
    [pscustomobject] @{
        Pattern = '^\.cargo_vcs_info\.json$'
        Category = 'provenance'
        Reason = 'The commit the tarball was built from, written by cargo. It is the only link from a published version back to a tree.'
    },
    [pscustomobject] @{
        Pattern = '^\.cargo/config\.toml$'
        Category = 'configuration'
        Reason = 'MSRV-aware resolution for anyone who builds the unpacked crate in place, which is section 9s canary. Inert for a registry consumer: cargo discovers configuration from the consumers working directory, not from a dependencys. Section 7 argues this at length.'
    },
    [pscustomobject] @{
        Pattern = '^README\.md$'
        Category = 'documentation'
        Reason = 'The crates.io front page, named by the readme key. src/lib.rs also pulls it in with include_str!, so every snippet in it is a doctest and the file is load-bearing for `cargo test --doc`.'
    },
    [pscustomobject] @{
        Pattern = '^(CHANGELOG|COMPATIBILITY|DIVERGENCES)\.md$'
        Category = 'documentation'
        Reason = 'Linked from README.md and from the crate documentation. A compatibility claim whose evidence and exceptions are only in the repository is not a claim the reader of a published crate can check.'
    },
    [pscustomobject] @{
        Pattern = '^src/locales/generated\.rs$'
        Category = 'generated-source'
        Reason = 'The 59 built-in locale definitions, generated from locale/*.json and committed. Section 1 forbids a build script that reads ../locale, so the table has to ship as source.'
    },
    [pscustomobject] @{
        Pattern = '^src/[a-z_]+\.rs$'
        Category = 'library'
        Reason = 'The library itself.'
    },
    [pscustomobject] @{
        Pattern = '^tests/'
        Category = 'tests'
        Reason = 'Section 9 requires the packaged crate to build and test independently, so the suite ships with it. No test reads fixtures/ at run time -- the generated tests carry every oracle value inline -- which is what makes that possible with fixtures/ excluded.'
    },
    [pscustomobject] @{
        Pattern = '^benches/format\.rs$'
        Category = 'benches'
        Reason = 'Buildable from the unpacked crate under the criterion dev-dependency the shipped lockfile pins. It measures the public API only, so it is also an example of using it.'
    }
)

# Paths that must never appear, each with the reason the exclusion exists. This
# duplicates part of Cargo.tomls `exclude` on purpose: the rule table above would
# also reject these, but a generic "unclassified path" failure would not say which
# deliberate exclusion had stopped working.
$ForbiddenPatterns = @(
    [pscustomobject] @{
        Pattern = '^fixtures/'
        Reason = 'The fixture corpora are sized in millions of cases and would push the package past the crates.io limit.'
    },
    [pscustomobject] @{
        Pattern = '^tools/'
        Reason = 'The JavaScript generators and the differential runner are verification tooling, not distribution.'
    },
    [pscustomobject] @{
        Pattern = '^ci/'
        Reason = 'The gate scripts are verification tooling, and ci/ now also holds the archived artifacts.'
    },
    [pscustomobject] @{
        Pattern = '^src/bin/'
        Reason = 'Shipping dump_cases would make `cargo install d3-format` install a differential harness from a formatting library.'
    },
    [pscustomobject] @{
        Pattern = '^rust-toolchain\.toml$'
        Reason = 'rustup resolves it by walking up from the build directory, so it would pin an unpacked-crate consumer to 1.98.1 and defeat the canary that the package builds under the consumers own toolchain.'
    },
    [pscustomobject] @{
        Pattern = '\.(ps1|mjs|jsonl)$'
        Reason = 'No PowerShell gate, JavaScript tool or fixture corpus has any business in a published Rust crate; this catches one arriving by a path the rules above do not name.'
    }
)

function Get-CommandOutput {
    <#
        As the other collectors: a native command's exit code does not stop a
        PowerShell script on its own, and $ErrorActionPreference does not cover
        native exit codes on Windows PowerShell 5.1.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)] [string] $Command,
        [Parameter(Mandatory = $true)] [AllowEmptyCollection()] [string[]] $Arguments
    )

    $captured = & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Command $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
    , @($captured)
}

function Get-WorktreeChanges {
    <#
        Every uncommitted path in the repository, tracked or not, one per output
        object. Untracked counts: a file HEAD does not contain is as much a
        difference from it as a modified one, and `cargo package` treats the two the
        same way.

        git is invoked here rather than through Get-CommandOutput deliberately. That
        helper returns the whole output as one array-valued object -- which is what
        its other callers want, since they index [0] or test .Count -gt 0, but a
        caller that needs the paths themselves gets an array holding an array and
        renders it as "System.Object[]". Normalizing through @() and dropping blank
        lines makes the shape the same for none, one and many.

        Duplicated verbatim in ci/parity/collect-differential.ps1 and
        ci/benchmarks/collect-baseline.ps1, as Get-CommandOutput already is, so each
        collector stays runnable on its own.
    #>

    $status = & git 'status' '--porcelain' '--untracked-files=normal'
    if ($LASTEXITCODE -ne 0) {
        throw "git status --porcelain --untracked-files=normal failed with exit code $LASTEXITCODE"
    }
    @($status) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
}

function Assert-CollectedFromACommit {
    <#
        The clean-tree precondition every collector applies before it measures
        anything and again before it writes the result.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)] [AllowEmptyCollection()] [string[]] $Changes,
        [Parameter(Mandatory = $true)] [string] $When
    )

    if ($Changes.Count -eq 0) {
        return
    }

    # Built by appending rather than as one nested literal: PowerShell does not
    # flatten an array written inside an array literal, and -join would render the
    # inner one as "System.Object[]".
    # ${...} as in ci/parity/assert-agreement.ps1: PowerShell reads `$When:` as a
    # drive-qualified variable and refuses to parse the file at all.
    $message = @("the working tree has $($Changes.Count) uncommitted path(s) ${When}:")
    $message += @($Changes | Select-Object -First 20 | ForEach-Object { "  $_" })
    if ($Changes.Count -gt 20) {
        $message += "  ... and $($Changes.Count - 20) more"
    }
    $message += @(
        '',
        'Release evidence names the commit it measured, and the only reason to name one',
        'is so a reader can check it out and reproduce the result. This tree is that',
        'commit plus changes no commit holds, so the artifact would point at nothing.',
        'Commit or stash the work and rerun.',
        '',
        '-AllowDirty overrides this for local experimentation and marks the artifact,',
        'which is then a note to yourself rather than release evidence.'
    )

    throw ($message -join [Environment]::NewLine)
}

$crateRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

if ([string]::IsNullOrEmpty($OutputPath)) {
    $OutputPath = Join-Path $PSScriptRoot 'package-listing.json'
}
# Made absolute before anything changes directory. [System.IO.File]::WriteAllText
# resolves a relative path against the *process* working directory, which is not the
# one Push-Location moves, so a caller-supplied relative path would otherwise land
# somewhere neither the caller nor this script expects.
$OutputPath = [System.IO.Path]::GetFullPath(
    [System.IO.Path]::Combine((Get-Location -PSProvider FileSystem).ProviderPath, $OutputPath))

Push-Location -LiteralPath $crateRoot
try {
    $manifest = Get-Content -LiteralPath (Join-Path $crateRoot 'Cargo.toml') -Raw
    if ($manifest -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
        throw 'Cargo.toml has no package version this script can read'
    }
    $version = $Matches[1]
    $cratePath = Join-Path $crateRoot "target/package/d3-format-$version.crate"

    # Read before the run and re-read after it, so the archive cannot name a commit
    # that was not HEAD for the whole collection.
    $commit = (Get-CommandOutput -Command 'git' -Arguments @('rev-parse', 'HEAD'))[0]

    Write-Host 'd3-format package listing'
    Write-Host "  version: $version"
    Write-Host "  commit:  $commit"
    Write-Host "  crate:   $cratePath"
    Write-Host "  output:  $OutputPath"

    # Checked before anything is built, so a dirty tree costs a second rather than a
    # package and a review of it.
    if (-not $AllowDirty) {
        Assert-CollectedFromACommit -Changes @(Get-WorktreeChanges) -When 'before the collection started'
    }

    # The other half of "this listing describes the named commit", and the half the
    # refusal above cannot see. `cargo package` reads the working tree rather than
    # the index, so a tracked file carrying CRLF changes the sha256 recorded below --
    # while git status stays silent, because a path git itself checked out as CRLF
    # still matches the stat data cached in the index. That is not hypothetical: it
    # is how the first archived hash came to be one no fresh clone could reproduce.
    #
    # Deliberately not waived by -AllowDirty. That switch says "measure work in
    # progress and mark the artifact"; it does not say "measure bytes the checkout
    # rule forbids", and there is no version of this collection for which a CRLF
    # packaged file is the intended input.
    & (Join-Path (Split-Path -Parent $PSScriptRoot) 'assert-line-endings.ps1')

    $startedUtc = (Get-Date).ToUniversalTime()

    if ($SkipPackage) {
        if (-not (Test-Path -LiteralPath $cratePath)) {
            throw "-SkipPackage was given but $cratePath does not exist"
        }
        Write-Host ''
        Write-Host '==> reusing the existing .crate (-SkipPackage)'
    }
    else {
        # Removing it first is what makes the freshness check below meaningful: an
        # aborted run must not leave a tarball that looks like this run's.
        if (Test-Path -LiteralPath $cratePath) {
            Remove-Item -LiteralPath $cratePath -Force
        }

        $packageArguments = @('package', '--locked')
        if ($AllowDirty) {
            $packageArguments += '--allow-dirty'
        }

        Write-Host ''
        Write-Host "==> cargo $($packageArguments -join ' ')"
        & cargo @packageArguments
        if ($LASTEXITCODE -ne 0) {
            throw "cargo $($packageArguments -join ' ') failed with exit code $LASTEXITCODE"
        }
    }

    if (-not (Test-Path -LiteralPath $cratePath)) {
        throw "cargo package exited zero but wrote no tarball to $cratePath"
    }
    $crateFile = Get-Item -LiteralPath $cratePath
    if (-not $SkipPackage -and $crateFile.LastWriteTimeUtc -lt $startedUtc.AddMinutes(-1)) {
        throw "$cratePath was written at $($crateFile.LastWriteTimeUtc), before this run started at $startedUtc"
    }

    # Second, independent rendering of the same decision. cargo derives it from the
    # same source-file walk but never compares the two, so a divergence would mean
    # the tarball is not the thing `cargo package --list` describes.
    $listArguments = @('package', '--list', '--locked')
    if ($AllowDirty) {
        $listArguments += '--allow-dirty'
    }
    Write-Host ''
    Write-Host "==> cargo $($listArguments -join ' ')"
    $listed = @(Get-CommandOutput -Command 'cargo' -Arguments $listArguments |
        ForEach-Object { $_.Trim() } |
        Where-Object { $_.Length -gt 0 } |
        ForEach-Object { $_ -replace '\\', '/' } |
        Sort-Object)

    # tar ships with Windows 10 1803 and later and with every CI image this project
    # uses. Reading the tarball rather than target/package/<name>/ matters: that
    # directory is cargo's own verification scratch space and nothing guarantees it
    # still corresponds to the tarball on disk.
    $extractRoot = Join-Path $crateRoot "target/package-listing"
    if (Test-Path -LiteralPath $extractRoot) {
        Remove-Item -LiteralPath $extractRoot -Recurse -Force
    }
    New-Item -ItemType Directory -Path $extractRoot -Force | Out-Null

    Write-Host ''
    Write-Host "==> tar -xzf $cratePath"
    & tar '-xzf' $cratePath '-C' $extractRoot
    if ($LASTEXITCODE -ne 0) {
        throw "tar -xzf $cratePath failed with exit code $LASTEXITCODE"
    }

    $unpacked = Join-Path $extractRoot "d3-format-$version"
    if (-not (Test-Path -LiteralPath $unpacked)) {
        throw "the tarball did not contain a d3-format-$version/ directory"
    }
    $prefixLength = $unpacked.Length + 1

    $files = @()
    $uncompressedBytes = 0
    foreach ($item in Get-ChildItem -LiteralPath $unpacked -Recurse -File -Force | Sort-Object FullName) {
        $relative = $item.FullName.Substring($prefixLength) -replace '\\', '/'

        foreach ($forbidden in $ForbiddenPatterns) {
            if ($relative -match $forbidden.Pattern) {
                throw (@(
                    "a deliberately excluded path is in the package: $relative",
                    "  matched : $($forbidden.Pattern)",
                    "  because : $($forbidden.Reason)",
                    '',
                    'Check the exclude list in Cargo.toml against section 7 before changing anything here.'
                ) -join [Environment]::NewLine)
            }
        }

        $matched = @($PackageRules | Where-Object { $relative -match $_.Pattern })
        if ($matched.Count -eq 0) {
            throw (@(
                "an unclassified file is in the package: $relative",
                '',
                'Every packaged path needs a rule in $PackageRules saying what it is and why a',
                'consumer needs it. Either add the file to the exclude list in Cargo.toml, or add',
                'the rule -- deliberately, because the rule is the review.'
            ) -join [Environment]::NewLine)
        }
        if ($matched.Count -gt 1) {
            throw (@(
                "$relative matches $($matched.Count) rules: $(($matched | ForEach-Object { $_.Pattern }) -join ', ')",
                'Two rules claiming one file means the table has stopped describing the package.'
            ) -join [Environment]::NewLine)
        }

        $uncompressedBytes += $item.Length
        $files += [ordered] @{
            path = $relative
            bytes = $item.Length
            sha256 = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            category = $matched[0].Category
        }
    }

    $packaged = @($files | ForEach-Object { $_.path } | Sort-Object)
    $onlyInTarball = @($packaged | Where-Object { $listed -notcontains $_ })
    $onlyInListing = @($listed | Where-Object { $packaged -notcontains $_ })
    if ($onlyInTarball.Count -ne 0 -or $onlyInListing.Count -ne 0) {
        throw (@(
            'the tarball and `cargo package --list` disagree.',
            "  only in the tarball : $(if ($onlyInTarball.Count) { $onlyInTarball -join ', ' } else { '(none)' })",
            "  only in --list      : $(if ($onlyInListing.Count) { $onlyInListing -join ', ' } else { '(none)' })"
        ) -join [Environment]::NewLine)
    }

    if ($crateFile.Length -ge $CratesIoMaxBytes) {
        throw "the package is $($crateFile.Length) bytes, at or above the crates.io limit of $CratesIoMaxBytes"
    }

    $byCategory = [ordered] @{}
    foreach ($rule in $PackageRules) {
        if (-not $byCategory.Contains($rule.Category)) {
            $byCategory[$rule.Category] = [ordered] @{ files = 0; bytes = 0; reasons = @() }
        }
        $byCategory[$rule.Category].reasons += $rule.Reason
    }
    foreach ($file in $files) {
        $byCategory[$file.category].files += 1
        $byCategory[$file.category].bytes += $file.bytes
    }

    Write-Host ''
    foreach ($category in $byCategory.Keys) {
        $entry = $byCategory[$category]
        Write-Host ("    {0,-17} {1,3} file(s), {2,8:n0} bytes" -f $category, $entry.files, $entry.bytes)
    }
    $totals = @('total', $files.Count, $uncompressedBytes, $crateFile.Length)
    Write-Host ("    {0,-17} {1,3} file(s), {2,8:n0} bytes uncompressed, {3:n0} compressed" -f $totals)

    # Re-checked now rather than only at the start: this is the value the archive
    # records, and recording it after asserting it is what makes git_worktree_dirty
    # false an enforced property of the run instead of a claim about its first
    # second.
    $changes = @(Get-WorktreeChanges)
    if (-not $AllowDirty) {
        Assert-CollectedFromACommit -Changes $changes -When 'after the collection finished'
    }
    $commitAfter = (Get-CommandOutput -Command 'git' -Arguments @('rev-parse', 'HEAD'))[0]
    if ($commitAfter -ne $commit) {
        throw "HEAD moved from $commit to $commitAfter during the collection, so this listing measures neither"
    }

    $excludeList = @()
    if ($manifest -match '(?m)^exclude\s*=\s*\[([^\]]*)\]') {
        $excludeList = @($Matches[1] -split ',' |
            ForEach-Object { $_.Trim().Trim('"') } |
            Where-Object { $_.Length -gt 0 })
    }

    $archive = [ordered] @{
        schema = 'd3-format/package-listing/1'
        generated_utc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
        regenerate_with = 'pwsh ci/package/collect-listing.ps1  (run from rust/)'
        package = [ordered] @{
            name = 'd3-format'
            version = $version
            file = "d3-format-$version.crate"
            sha256 = (Get-FileHash -LiteralPath $cratePath -Algorithm SHA256).Hash.ToLowerInvariant()
            compressed_bytes = $crateFile.Length
            uncompressed_bytes = $uncompressedBytes
            file_count = $files.Count
            crates_io_max_bytes = $CratesIoMaxBytes
            headroom_bytes = $CratesIoMaxBytes - $crateFile.Length
        }
        provenance = [ordered] @{
            git_commit = $commit
            git_worktree_dirty = $changes.Count -gt 0
            allow_dirty = [bool] $AllowDirty
            cargo = (Get-CommandOutput -Command 'cargo' -Arguments @('--version'))[0]
            rustc = (Get-CommandOutput -Command 'rustc' -Arguments @('--version'))[0]
        }
        excluded = [ordered] @{
            manifest_exclude = $excludeList
            asserted_absent = @($ForbiddenPatterns | ForEach-Object {
                    [ordered] @{ pattern = $_.Pattern; reason = $_.Reason }
                })
        }
        categories = $byCategory
        files = $files
        notes = @(
            'Read out of the tarball, not out of the working tree: the per-file sha256 values are the bytes a consumer downloads.',
            'The package sha256 changes with every commit even when no source does, because cargo writes the commit into .cargo_vcs_info.json. It identifies this build rather than this content, so it is reproducible from the named commit and from no other: package that commit again and the bytes are identical.',
            'provenance.git_commit names the commit whose tree was measured. This artifact records that measurement and therefore lands in a later commit, which is expected rather than a defect. What makes it evidence is provenance.git_worktree_dirty being false: the tree measured was exactly the named commit, so a reader can check that commit out and reproduce this. The collector refuses a dirty tree, so the false value is enforced rather than asserted; -AllowDirty waives the refusal and says so in provenance.allow_dirty.',
            'Every path here matched exactly one rule in collect-listing.ps1, and each rule carries the reason a consumer needs the file. A packaged path that matches no rule fails the collector, so this listing cannot go stale by accident.',
            'The listing is a record, not a gate that CI runs. The executable packaging gate is `cargo package --locked` inside ci/check.ps1 -Profile Pinned, and .github/workflows/publish-rust.yml runs this collector before it uploads anything.'
        )
    }

    $json = $archive | ConvertTo-Json -Depth 24 -Compress
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    $text = ($json -replace "`r`n", "`n").TrimEnd("`n") + "`n"
    [System.IO.File]::WriteAllText($OutputPath, $text, $utf8NoBom)

    Write-Host ''
    Write-Host "OK: wrote $OutputPath ($([math]::Round($text.Length / 1kb, 1)) KiB)"
}
finally {
    Pop-Location
}

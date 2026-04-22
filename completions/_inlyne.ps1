
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'inlyne' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'inlyne'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'inlyne' {
            [CompletionResult]::new('-t', '-t', [CompletionResultType]::ParameterName, 'Theme to use when rendering')
            [CompletionResult]::new('--theme', '--theme', [CompletionResultType]::ParameterName, 'Theme to use when rendering')
            [CompletionResult]::new('-d', '-d', [CompletionResultType]::ParameterName, 'Enable decorations')
            [CompletionResult]::new('--decorations', '--decorations', [CompletionResultType]::ParameterName, 'Enable decorations')
            [CompletionResult]::new('-s', '-s', [CompletionResultType]::ParameterName, 'Factor to scale rendered file by [default: OS defined window scale factor]')
            [CompletionResult]::new('--scale', '--scale', [CompletionResultType]::ParameterName, 'Factor to scale rendered file by [default: OS defined window scale factor]')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Configuration file to use')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Configuration file to use')
            [CompletionResult]::new('-w', '-w', [CompletionResultType]::ParameterName, 'Maximum width of page in pixels')
            [CompletionResult]::new('--page-width', '--page-width', [CompletionResultType]::ParameterName, 'Maximum width of page in pixels')
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Position of the opened window <x>,<y>')
            [CompletionResult]::new('--win-pos', '--win-pos', [CompletionResultType]::ParameterName, 'Position of the opened window <x>,<y>')
            [CompletionResult]::new('--win-size', '--win-size', [CompletionResultType]::ParameterName, 'Size of the opened window <width>x<height>')
            [CompletionResult]::new('--font-size', '--font-size', [CompletionResultType]::ParameterName, 'Font size [default: 16]')
            [CompletionResult]::new('--line-height', '--line-height', [CompletionResultType]::ParameterName, 'Line height [default: 1.1]')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('view', 'view', [CompletionResultType]::ParameterValue, 'View a markdown file with inlyne')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Configuration related things')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'inlyne;view' {
            [CompletionResult]::new('-t', '-t', [CompletionResultType]::ParameterName, 'Theme to use when rendering')
            [CompletionResult]::new('--theme', '--theme', [CompletionResultType]::ParameterName, 'Theme to use when rendering')
            [CompletionResult]::new('-d', '-d', [CompletionResultType]::ParameterName, 'Enable decorations')
            [CompletionResult]::new('--decorations', '--decorations', [CompletionResultType]::ParameterName, 'Enable decorations')
            [CompletionResult]::new('-s', '-s', [CompletionResultType]::ParameterName, 'Factor to scale rendered file by [default: OS defined window scale factor]')
            [CompletionResult]::new('--scale', '--scale', [CompletionResultType]::ParameterName, 'Factor to scale rendered file by [default: OS defined window scale factor]')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Configuration file to use')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Configuration file to use')
            [CompletionResult]::new('-w', '-w', [CompletionResultType]::ParameterName, 'Maximum width of page in pixels')
            [CompletionResult]::new('--page-width', '--page-width', [CompletionResultType]::ParameterName, 'Maximum width of page in pixels')
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Position of the opened window <x>,<y>')
            [CompletionResult]::new('--win-pos', '--win-pos', [CompletionResultType]::ParameterName, 'Position of the opened window <x>,<y>')
            [CompletionResult]::new('--win-size', '--win-size', [CompletionResultType]::ParameterName, 'Size of the opened window <width>x<height>')
            [CompletionResult]::new('--font-size', '--font-size', [CompletionResultType]::ParameterName, 'Font size [default: 16]')
            [CompletionResult]::new('--line-height', '--line-height', [CompletionResultType]::ParameterName, 'Line height [default: 1.1]')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'inlyne;config' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('open', 'open', [CompletionResultType]::ParameterValue, 'Opens the configuration file in the default text editor')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'inlyne;config;open' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'inlyne;config;help' {
            [CompletionResult]::new('open', 'open', [CompletionResultType]::ParameterValue, 'Opens the configuration file in the default text editor')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'inlyne;config;help;open' {
            break
        }
        'inlyne;config;help;help' {
            break
        }
        'inlyne;help' {
            [CompletionResult]::new('view', 'view', [CompletionResultType]::ParameterValue, 'View a markdown file with inlyne')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Configuration related things')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'inlyne;help;view' {
            break
        }
        'inlyne;help;config' {
            [CompletionResult]::new('open', 'open', [CompletionResultType]::ParameterValue, 'Opens the configuration file in the default text editor')
            break
        }
        'inlyne;help;config;open' {
            break
        }
        'inlyne;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}

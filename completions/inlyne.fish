# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_inlyne_global_optspecs
	string join \n t/theme= d/decorations= s/scale= c/config= w/page-width= p/win-pos= win-size= h/help V/version
end

function __fish_inlyne_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_inlyne_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_inlyne_using_subcommand
	set -l cmd (__fish_inlyne_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c inlyne -n "__fish_inlyne_needs_command" -s t -l theme -d 'Theme to use when rendering' -r -f -a "auto\t''
dark\t''
light\t''"
complete -c inlyne -n "__fish_inlyne_needs_command" -s d -l decorations -d 'Enable decorations' -r -f -a "true\t''
false\t''"
complete -c inlyne -n "__fish_inlyne_needs_command" -s s -l scale -d 'Factor to scale rendered file by [default: OS defined window scale factor]' -r
complete -c inlyne -n "__fish_inlyne_needs_command" -s c -l config -d 'Configuration file to use' -r -F
complete -c inlyne -n "__fish_inlyne_needs_command" -s w -l page-width -d 'Maximum width of page in pixels' -r
complete -c inlyne -n "__fish_inlyne_needs_command" -s p -l win-pos -d 'Position of the opened window <x>,<y>' -r
complete -c inlyne -n "__fish_inlyne_needs_command" -l win-size -d 'Size of the opened window <width>x<height>' -r
complete -c inlyne -n "__fish_inlyne_needs_command" -s h -l help -d 'Print help'
complete -c inlyne -n "__fish_inlyne_needs_command" -s V -l version -d 'Print version'
complete -c inlyne -n "__fish_inlyne_needs_command" -a "view" -d 'View a markdown file with inlyne'
complete -c inlyne -n "__fish_inlyne_needs_command" -a "config" -d 'Configuration related things'
complete -c inlyne -n "__fish_inlyne_needs_command" -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c inlyne -n "__fish_inlyne_using_subcommand view" -s t -l theme -d 'Theme to use when rendering' -r -f -a "auto\t''
dark\t''
light\t''"
complete -c inlyne -n "__fish_inlyne_using_subcommand view" -s d -l decorations -d 'Enable decorations' -r -f -a "true\t''
false\t''"
complete -c inlyne -n "__fish_inlyne_using_subcommand view" -s s -l scale -d 'Factor to scale rendered file by [default: OS defined window scale factor]' -r
complete -c inlyne -n "__fish_inlyne_using_subcommand view" -s c -l config -d 'Configuration file to use' -r -F
complete -c inlyne -n "__fish_inlyne_using_subcommand view" -s w -l page-width -d 'Maximum width of page in pixels' -r
complete -c inlyne -n "__fish_inlyne_using_subcommand view" -s p -l win-pos -d 'Position of the opened window <x>,<y>' -r
complete -c inlyne -n "__fish_inlyne_using_subcommand view" -l win-size -d 'Size of the opened window <width>x<height>' -r
complete -c inlyne -n "__fish_inlyne_using_subcommand view" -s h -l help -d 'Print help'
complete -c inlyne -n "__fish_inlyne_using_subcommand config; and not __fish_seen_subcommand_from open help" -s h -l help -d 'Print help'
complete -c inlyne -n "__fish_inlyne_using_subcommand config; and not __fish_seen_subcommand_from open help" -f -a "open" -d 'Opens the configuration file in the default text editor'
complete -c inlyne -n "__fish_inlyne_using_subcommand config; and not __fish_seen_subcommand_from open help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c inlyne -n "__fish_inlyne_using_subcommand config; and __fish_seen_subcommand_from open" -s h -l help -d 'Print help'
complete -c inlyne -n "__fish_inlyne_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "open" -d 'Opens the configuration file in the default text editor'
complete -c inlyne -n "__fish_inlyne_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c inlyne -n "__fish_inlyne_using_subcommand help; and not __fish_seen_subcommand_from view config help" -f -a "view" -d 'View a markdown file with inlyne'
complete -c inlyne -n "__fish_inlyne_using_subcommand help; and not __fish_seen_subcommand_from view config help" -f -a "config" -d 'Configuration related things'
complete -c inlyne -n "__fish_inlyne_using_subcommand help; and not __fish_seen_subcommand_from view config help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c inlyne -n "__fish_inlyne_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "open" -d 'Opens the configuration file in the default text editor'

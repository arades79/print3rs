static FULL_HELP: &str = "    
Anything entered not matching one of the following commands is uppercased and sent to
the printer for it to interpret.

Some commands cannot be ran until a printer is connected.

Multiple Gcodes can be sent on the same line by separating with ';'.

Arguments with ? are optional.

Available commands:
help         <command?>       display this message or details for specified command
version                       display version
clear                         clear all text on the screen
printerinfo                   display any information found about the connected printer
print        <file>           send gcodes from file to printer
log          <name> <pattern> begin logging parsed output from printer
repeat       <name> <gcodes>  run the given gcodes in a loop until stop
stop         <name>           stop an active print, log, or repeat
macro        <name> <gcodes>  make an alias for a set of gcodes
delmacro     <name>           remove an existing alias for set of gcodes
macros                        list existing command aliases and contents           
connect      <proto?> <args?> connect to a device using protocol and args, or attempt to autoconnect
disconnect                    disconnect from printer
quit                          exit program
\n";

pub fn help(command: &str) -> &'static str {
    let command = command.trim();

    match command {
        "send" => "send: explicitly send one or more commands (separated by gcode comment character `;`) commands to the printer, no uppercasing or additional parsing is performed. This can be used to send commands to the printer that would otherwise be detected as a console command.\n",
        "print" => "print: execute every line of G-code sequentially from the given file. The print job is added as a task which runs in the background with the filename as the task name. Other commands can be sent while a print is running, and a print can be stopped at any time with `stop`\n",
        "log" => "log: begin logging the specified pattern from the printer into a csv with the `name` given. This operation runs in the background and is added as a task which can be stopped with `stop`. The pattern given will be used to parse the logs, with values wrapped in `{}` being given a column of whatever is between the `{}`, and pulling a number in its place. If your pattern needs to include a literal `{` or `}`, double them up like `{{` or `}}` to have the parser read it as just a `{` or `}` in the output.\n",
        "repeat" => "repeat: repeat the given Gcodes (separated by gcode comment character `;`) in a loop until stopped. \n",
        "stop" => "stop: stops a task running in the background. All background tasks are required to have a name, thus this command can be used to stop them. Tasks can also stop themselves if they fail or can complete, after which running this will do nothing.\n",
        "connect" => "connect: Manually connect to a printer by specifying a protocol and some arguments. Arguments depend on protocol. For serial connection specify its path and optionally its baudrate. On windows this looks like `connect serial COM3 115200`, on linux more like `connect serial /dev/tty/ACM0 250000`. This does not test if the printer is capable of responding to messages, it will only open the port. Specifying no arguments will attempt autoconnection using serial.\n",
        "disconnect" => "disconnect: disconnect from the currently connected printer. All active tasks will be stopped\n",
        "macro" => "create a case-insensitve alias to some set of gcodes, even containing other macros recursively to build up complex sets of builds with a single word. Macro names cannot start with G,T,M,N, or D to avoid conflict with Gcodes, and cannot have any non-alphanumeric characters. commands in a macro are separated by ';', and macros can be used anywhere Gcodes are passed, including repeat commands and sends.\n",
        _ => FULL_HELP,
    }
}

#[cfg(test)]
#[test]
fn test_help() {
    assert_eq!(
        help("fhsfhebfuhubfhiedbudbhfjuygehrjfuygrhejdnbfgytu8r7y4jnb5thuif9d8s7wyhj3m4nrb"),
        FULL_HELP
    );
    assert_ne!(help("print"), help("stop"));
}

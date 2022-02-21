# Print and ads new line
def println [
    ...message: string # Message to be printed Joined with spaces
] {
    echo ($message | str collect " ") ;char newline
}

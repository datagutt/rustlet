load("render.star", "render")

def main(config):
    return render.Root(
        child = render.Box(
            color = "#000033",
            child = render.Row(
                expanded = True,
                main_align = "center",
                cross_align = "center",
                children = [
                    render.Text("Hello!"),
                ],
            ),
        ),
    )

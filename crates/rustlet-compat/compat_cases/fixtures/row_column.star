load("render.star", "render")

def main(config):
    return render.Root(
        child = render.Row(
            expanded = True,
            main_align = "space_evenly",
            cross_align = "center",
            children = [
                render.Box(width = 12, height = 20, color = "#f00"),
                render.Column(
                    main_align = "space_evenly",
                    cross_align = "center",
                    children = [
                        render.Box(width = 10, height = 6, color = "#0f0"),
                        render.Box(width = 18, height = 4, color = "#00f"),
                    ],
                ),
                render.Box(width = 6, height = 6, color = "#ff0"),
            ],
        ),
    )

function bench()
    categories = Dict{String, Vector{Float64}}()
    cat_c = Dict{String, Vector{Float64}}()
    cat_a = Dict{String, Int64}()
    cat_n = Dict{String, Int}()

    open("target/synthetic_1m.csv") do io
        readline(io) # header
        for line in eachline(io)
            parts = split(line, ',')
            if length(parts) >= 10 && parts[10] == "OK"
                cat = String(parts[5])
                va = parse(Int64, parts[2])
                vb = parse(Float64, parts[3])
                vc = parse(Float64, parts[4])

                cat_n[cat] = get(cat_n, cat, 0) + 1
                cat_a[cat] = get(cat_a, cat, 0) + va
                push!(get!(categories, cat, Float64[]), vb)
                push!(get!(cat_c, cat, Float64[]), vc)
            end
        end
    end

    for k in sort(collect(keys(cat_n)))
        mb = sum(categories[k]) / cat_n[k]
        mc = sum(cat_c[k]) / cat_n[k]
        println("$k: mean_b=$(round(mb, digits=2)), mean_c=$(round(mc, digits=2)), total_a=$(cat_a[k]), n=$(cat_n[k])")
    end
end
bench()

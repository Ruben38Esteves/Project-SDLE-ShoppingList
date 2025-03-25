import React, { useEffect, useState } from 'react';
import { Form, Route, Routes, useParams } from 'react-router-dom';
import axios from 'axios';

const fetchGeneratedId = async () => {
    try {
        const response = await axios.get('http://localhost:5000/generate_id');
        return response.data;
    } catch (error) {
        console.error('Error generating ID:', error);
        return null;
    }
};

// Define the type for list.json data
type ListData = {
    item_name: string;
    bought: number;
    target: number;
    replica: string;
    timestamp: number;
    deleted: boolean;
};

type ListJson = {
    [key: string]: {
        c: { replica: string; timestamp: number }[];
        s: ListData[];
    };
};


const Home: React.FC = () => {
    return <h1>Home Page</h1>;
};

const ListView: React.FC = () => {
    const { id } = useParams<{ id: string }>();
    const [list, setList] = useState<ListData[] | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [name, setName] = useState('');
    const [amount, setAmount] = useState('');
    const [inputValue, setInputValue] = useState<{ [key: string]: number }>({});
    const [userId, setUserId] = useState<string | null>(null);

    useEffect(() => {
        const getId = async () => {
            let storedId = localStorage.getItem('userId');
            if (!storedId) {
                storedId = await fetchGeneratedId() as string | null;
                if (storedId) {
                    localStorage.setItem('userId', storedId);
                }
            }
            setUserId(storedId);
        };

        getId();
    }, []);

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        const change = {
            type: "add",
            list_id: id,
            item_name: name,
            target: parseInt(amount),
            bought: 0,
            replica: userId
        }
        try {
            await axios.post('http://localhost:5000/changes', change);
            alert('Change submitted successfully');
        } catch (error) {
            console.error('Error submitting change:', error);
            alert('Failed to submit change');
        }
    };

    const handleRemove = async (itemName: string) => {
        const change = {
            type: "remove",
            list_id: id,
            item_name: itemName,
            target: 0,
            bought: 0,
            replica: userId
        };
        try {
            await axios.post('http://localhost:5000/changes', change);
            alert('Item removed successfully');
            //setList(list?.filter(item => item.item_name !== itemName) || null);
        } catch (error) {
            console.error('Error removing item:', error);
            alert('Failed to remove item');
        }
    };

    // Handle input changes
    const handleInputChange = (itemName: string, value: string) => {
        const numericValue = parseInt(value, 10) || 0;
        setInputValue((prevState) => ({
            ...prevState,
            [itemName]: numericValue,
        }));
    };

    const handleUpdate = async (itemName: string, amount:number, old_bought: number, old_target: number) => {
        const value = inputValue[itemName] || 0;
        let target = old_target
        let bought = old_bought
        if(value >=  0){
            target += value
        }else{
            bought -= value
        }
        const change = {
            type: "update",
            list_id: id,
            item_name: itemName,
            target: target,
            bought: bought,
            replica: userId
        };
        try {
            await axios.post('http://localhost:5000/changes', change);
            alert('Item removed successfully');
            //setList(list?.filter(item => item.item_name !== itemName) || null);
        } catch (error) {
            console.error('Error removing item:', error);
            alert('Failed to remove item');
        }
    };

    useEffect(() => {
        const fetchList = async () => {
            try {
                let request_url = "http://localhost:5000/list.json/" + id
                // Fetch the entire JSON
                const response = await axios.get<ListJson>(request_url);
                const data = response.data;
                // Get the first key in the JSON (since it's dynamic)
                const firstKey = Object.keys(data)[0];

                // Check if the id from the URL matches the first key
                if (firstKey === id) {
                    setList(data[firstKey].s); // Set the shopping list from the "s" field
                    setError(null);
                } else {
                    setList(null);
                    setError('No list associated with this ID.');
                }
            } catch (err) {
                console.error(err);
                setError('Failed to fetch the list.');
            }
        };

        if (id) {
            fetchList();
        }
    }, [id]);

    return (
        <div>
            {error && <h2>{error}</h2>}
            {list && list.length > 0 ? (
                list.map((item, index) => (
                    !item.deleted && (
                        <li key={index}>
                            <p><strong>{item.item_name}</strong></p>
                            <button onClick={() => handleRemove(item.item_name)}>Remove</button>
                            <p>
                                Need: {item.target - item.bought}
                            </p>
                            <form onSubmit={(e) => handleUpdate(item.item_name, e, item.bought, item.target )}>
                                <label>
                                    Change amount needed:
                                    <input
                                        type="number"
                                        value={inputValue[item.item_name] || ''}
                                        onChange={(e) =>
                                            handleInputChange(item.item_name, e.target.value)
                                        }
                                        required
                                    />
                                </label>
                                <button type="submit">Update</button>
                            </form>
                        </li>
                    )
                ))
            ) : (
                <p>No items found or loading...</p>
            )}
            <p>Insert new Item</p>
            <form onSubmit={handleSubmit} className="form-container">
                <label>
                    Enter name:
                    <input
                        type="text"
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                        pattern="[A-Za-z\s]+"
                        required
                    />
                </label>
                <label>
                    Enter amount:
                    <input
                        type="number"
                        value={amount}
                        onChange={(e) => setAmount(e.target.value)}
                        required
                    />
                </label>
                <input
                        type="hidden"
                        value={id}
                        onChange={(e) => setAmount(e.target.value)}
                        required
                    />
                <button type="submit">Submit</button>
            </form>
        </div>
    );
};

const App: React.FC = () => {
    return (
        <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/list/:id" element={<ListView />} />
        </Routes>
    );
};

export default App;
